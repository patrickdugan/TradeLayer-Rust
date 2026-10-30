// Separate opt-in entry; original queue METHODS, evaluators and wire stay exact.
include!("contract_orderbook_helpers_numeric_impl.rs");

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OrderbookHelperEntryV1 {
    MainSubgraph,
    OrderbookFirst,
}

impl ContractQueueSourceOwner {
    pub fn evaluate_orderbook_helper_instance<F>(
        &mut self,
        instance: u64,
        entry: OrderbookHelperEntryV1,
        call: &SourceCall,
        capability: &mut F,
    ) -> Result<SourceOutcome, SourceRuleError>
    where
        F: FnMut(&mut Self, &DependencyCall) -> Result<DependencyReturn, SourceRuleError>,
    {
        if ![
            "getAddressOrders",
            "cancelContractOrdersForSize",
            "adjustOrdersForAddress",
        ]
        .contains(&call.method.as_str())
        {
            return Err(SourceRuleError::UnknownMethod);
        }
        if call.arguments.len() > 8 {
            return Err(SourceRuleError::Capacity);
        }
        for value in &call.arguments {
            value.validate()?;
        }
        self.instance(instance)?;
        let before = self.clone();
        let result = self.orderbook_helper_run(instance, entry, call, capability);
        if let Err(Fault::Local(error) | Fault::Boundary(error)) = &result {
            let error = error.clone();
            *self = before;
            return Err(error);
        }
        if let Err(error) = self.checkpoint() {
            *self = before;
            return Err(error);
        }
        Ok(match result {
            Ok(v) => SourceOutcome::Returned(v),
            Err(Fault::Thrown(e)) => SourceOutcome::Thrown(e),
            _ => unreachable!(),
        })
    }
    fn orderbook_helper_run<F>(
        &mut self,
        instance: u64,
        entry: OrderbookHelperEntryV1,
        call: &SourceCall,
        capability: &mut F,
    ) -> Eval<V>
    where
        F: FnMut(&mut Self, &DependencyCall) -> Result<DependencyReturn, SourceRuleError>,
    {
        let key = book_helper_string(&self.instance(instance)?.key)?;
        let mut parsed = false;
        let mut book = self.load_book_status(instance, s(&key), &mut parsed)?;
        let result = match call.method.as_str() {
            "getAddressOrders" => self.orderbook_helper_address(&book, &call.arguments),
            "cancelContractOrdersForSize" => {
                self.orderbook_helper_cancel(&mut book, &call.arguments)
            }
            "adjustOrdersForAddress" => self.orderbook_helper_adjust(
                (instance, &key, parsed, entry),
                &mut book,
                &call.arguments,
                capability,
            ),
            _ => unreachable!(),
        };
        // loadOrderBook's parsed object aliases this exact receiver's cache.
        // Native source throws retain sort/splice/flag/margin prefixes, no save.
        if parsed {
            set_object(&mut self.instance_mut(instance)?.books, &key, book)?;
        }
        result
    }
    fn orderbook_helper_address(&self, book: &V, a: &[V]) -> Eval<V> {
        if !truth(book) {
            return Ok(V::Array(vec![]));
        }
        let side = if truth(&arg(a, 1)) { "sell" } else { "buy" };
        let value = prop(book, side)?;
        let rows = book_helper_array(&value, "filter")?;
        let mut found = vec![];
        for order in rows {
            if strict(&prop(order, "sender")?, &arg(a, 0)) {
                found.push(order.clone());
            }
        }
        Ok(V::Array(found))
    }
    fn orderbook_helper_cancel(&self, book: &mut V, a: &[V]) -> Eval<V> {
        if !truth(book) {
            return Ok(V::Array(vec![]));
        }
        let sell = truth(&arg(a, 3));
        let side = if sell { "sell" } else { "buy" };
        let value = prop(book, side)?;
        let mut rows = book_helper_array(&value, "sort")?.to_vec();
        let result = (|| -> Eval<V> {
            // Literal comparator: sell ascending, buy descending, despite comments.
            source_v8_stable_sort(&mut rows, |left, right| {
                let first = book_helper_number(&prop(left, "price")?)?;
                let second = book_helper_number(&prop(right, "price")?)?;
                let delta = if sell { first - second } else { second - first };
                Ok::<_, Fault>(
                    delta
                        .partial_cmp(&0.0)
                        .unwrap_or(core::cmp::Ordering::Equal),
                )
            })?;
            let mut remaining = BookHelperBn::new(&arg(a, 4))?;
            let mut cancelled = vec![];
            let mut i = 0;
            while i < rows.len() {
                if !strict(&prop(&rows[i], "sender")?, &arg(a, 0)) {
                    i += 1;
                    continue;
                }
                let amount = BookHelperBn::new(&prop(&rows[i], "amount")?)?;
                let cancel = book_helper_min(&amount, &remaining);
                cancelled.push(obj(&[
                    ("txid", prop(&rows[i], "txid")?),
                    ("amountCancelled", V::number(cancel.number())),
                    ("price", prop(&rows[i], "price")?),
                ]));
                remaining = remaining.op(cancel.clone(), 1)?;
                if matches!(
                    amount.compare(&cancel),
                    Some(core::cmp::Ordering::Less | core::cmp::Ordering::Equal)
                ) {
                    rows.remove(i);
                } else {
                    let next = amount.op(cancel, 1)?.number();
                    set_prop(&mut rows[i], "amount", V::number(next))?;
                    i += 1;
                }
                if matches!(
                    remaining.compare(&BookHelperBn::Finite(Decimal::zero())),
                    Some(core::cmp::Ordering::Less | core::cmp::Ordering::Equal)
                ) {
                    break;
                }
            }
            Ok(V::Array(cancelled))
        })();
        set_prop(book, side, V::Array(rows))?;
        result
    }
    fn orderbook_helper_adjust<F>(
        &mut self,
        context: (u64, &str, bool, OrderbookHelperEntryV1),
        book: &mut V,
        a: &[V],
        capability: &mut F,
    ) -> Eval<V>
    where
        F: FnMut(&mut Self, &DependencyCall) -> Result<DependencyReturn, SourceRuleError>,
    {
        let (instance, key, parsed, entry) = context;
        let contract = arg(a, 1);
        let contract_key = book_helper_string(&contract)?;
        let mut all = prop(book, "orderBooks")?;
        let loaded = prop(&all, &contract_key)?;
        let alias = truth(&loaded);
        let mut selected = if alias { loaded } else { empty_book() };
        let mut assigned = false;
        let result = (|| -> Eval<V> {
            book_helper_string(&arg(a, 0))?;
            book_helper_string(&contract)?;
            json_stringify(&arg(a, 3))?;
            let mut required = BookHelperBn::Finite(Decimal::zero());
            let mut total = BookHelperBn::Finite(Decimal::zero());
            for side in ["buy", "sell"] {
                let list = prop(&selected, side)?;
                let mut rows = book_helper_indexed_rows(&list)?;
                let side_result = (|| -> Eval<()> {
                    for i in (0..rows.len()).rev() {
                        let order = &mut rows[i];
                        let sender = prop(order, "sender")?;
                        let address = if truth(&sender) {
                            sender
                        } else {
                            prop(order, "address")?
                        };
                        if !strict(&address, &arg(a, 0)) {
                            continue;
                        }
                        let side_value = prop(order, "side")?;
                        let order_side = if truth(&side_value) {
                            side_value
                        } else {
                            s(if truth(&prop(order, "sell")?) {
                                "sell"
                            } else {
                                "buy"
                            })
                        };
                        let position = arg(a, 3);
                        let reduce = (strict(&order_side, &s("buy"))
                            && book_helper_number(&prop(&position, "contracts")?)? < 0.0)
                            || (strict(&order_side, &s("sell"))
                                && book_helper_number(&prop(&position, "contracts")?)? > 0.0);
                        if !strict(&prop(order, "initialReduce")?, &V::Bool(reduce)) {
                            book_helper_string(&prop(order, "txid")?)?;
                            book_helper_string(&prop(order, "initialReduce")?)?;
                            set_prop(order, "initialReduce", V::Bool(reduce))?;
                            if reduce {
                                book_helper_string(&prop(order, "txid")?)?;
                                book_helper_string(&prop(order, "initMargin")?)?;
                                return Err(book_helper_reference("TallyMap"));
                            }
                            required = required
                                .clone()
                                .op(BookHelperBn::new(&prop(order, "initMargin")?)?, 0)?;
                        }
                        if entry == OrderbookHelperEntryV1::MainSubgraph {
                            return Err(type_error(
                                "ContractRegistry.getInitialMargin is not a function",
                            ));
                        }
                        let arguments = vec![contract.clone(), prop(&position, "avgPrice")?];
                        // Publish the real alias prefix before the reached dependency.
                        let mut prefix = selected.clone();
                        set_prop(&mut prefix, side, book_helper_write_indexed(&list, &rows)?)?;
                        let mut roots = all.clone();
                        set_object(&mut roots, &contract_key, prefix)?;
                        let mut actual = book.clone();
                        set_prop(&mut actual, "orderBooks", roots)?;
                        if parsed {
                            set_object(&mut self.instance_mut(instance)?.books, key, actual)?;
                        }
                        let returned = capability(
                            self,
                            &DependencyCall {
                                target: "Contract.getInitialMargin".into(),
                                arguments,
                            },
                        )?;
                        let margin = match returned {
                            DependencyReturn::Immediate(v) | DependencyReturn::Pending(v) => v,
                            DependencyReturn::Throw(error)
                            | DependencyReturn::PendingThrow(error) => {
                                return Err(Fault::Thrown(error))
                            }
                        };
                        let expected = BookHelperBn::new(&margin)?
                            .op(BookHelperBn::new(&prop(&rows[i], "amount")?)?, 2)?
                            .q(8, crate::decimal::Rounding::HalfUp)?
                            .number();
                        if !strict(&prop(&rows[i], "marginUsed")?, &V::number(expected)) {
                            book_helper_string(&prop(&rows[i], "marginUsed")?)?;
                            book_helper_string(&prop(&rows[i], "txid")?)?;
                            set_prop(&mut rows[i], "marginUsed", V::number(expected))?;
                        }
                        total = total
                            .clone()
                            .op(BookHelperBn::new(&V::number(expected))?, 0)?;
                    }
                    Ok(())
                })();
                set_prop(
                    &mut selected,
                    side,
                    book_helper_write_indexed(&list, &rows)?,
                )?;
                side_result?;
            }
            if matches!(
                required.compare(&BookHelperBn::Finite(Decimal::zero())),
                Some(core::cmp::Ordering::Greater)
            ) {
                return Err(book_helper_reference("TallyMap"));
            }
            set_object(&mut all, &contract_key, selected.clone())?;
            assigned = true;
            Err(book_helper_reference("Orderbooks"))
        })();
        if alias || assigned {
            set_object(&mut all, &contract_key, selected)?;
            set_prop(book, "orderBooks", all)?;
        }
        result
    }
}
fn book_helper_reference(name: &str) -> Fault {
    Fault::Thrown(SourceException {
        name: "ReferenceError".into(),
        message: format!("{name} is not defined"),
    })
}
fn book_helper_array<'a>(value: &'a V, method: &str) -> Eval<&'a [V]> {
    if matches!(value, V::Null | V::Undefined) {
        prop(value, method)?;
    }
    match value {
        V::Array(rows) => Ok(rows),
        _ => Err(type_error(format!("orders.{method} is not a function"))),
    }
}
fn book_helper_indexed_rows(value: &V) -> Eval<Vec<V>> {
    // The source reads length first; missing length gives an empty reverse loop.
    let length = number(&prop(value, "length")?)?;
    if length.is_nan() || length <= 0.0 {
        return Ok(vec![]);
    }
    if !length.is_finite() || length > MAX_ROWS as f64 {
        return Err(SourceRuleError::Capacity.into());
    }
    (0..length.floor() as usize)
        .map(|i| prop(value, &i.to_string()))
        .collect()
}
fn book_helper_write_indexed(original: &V, rows: &[V]) -> Eval<V> {
    if matches!(original, V::Array(_)) {
        return Ok(V::Array(rows.to_vec()));
    }
    let mut value = original.clone();
    for (i, row) in rows.iter().enumerate() {
        let key = i.to_string();
        if prop(original, &key)? != *row {
            set_prop(&mut value, &key, row.clone())?;
        }
    }
    Ok(value)
}
fn book_helper_min(left: &BookHelperBn, right: &BookHelperBn) -> BookHelperBn {
    match left.compare(right) {
        None => BookHelperBn::Special(f64::NAN),
        Some(core::cmp::Ordering::Greater) => right.clone(),
        Some(core::cmp::Ordering::Equal)
            if matches!(left, BookHelperBn::NegativeZero)
                || matches!(right, BookHelperBn::NegativeZero) =>
        {
            BookHelperBn::NegativeZero
        }
        _ => left.clone(),
    }
}

fn book_helper_string(value: &V) -> Eval<String> {
    match value {
        V::Object(fields) if fields.iter().any(|(key, _)| key == "toString") => {
            Err(type_error("Cannot convert object to primitive value"))
        }
        V::Array(values) => values
            .iter()
            .map(|v| {
                if matches!(v, V::Null | V::Undefined) {
                    Ok(String::new())
                } else {
                    book_helper_string(v)
                }
            })
            .collect::<Eval<Vec<_>>>()
            .map(|v| v.join(",")),
        _ => text(value),
    }
}
fn book_helper_number(value: &V) -> Eval<f64> {
    if matches!(value, V::Object(_) | V::Array(_)) {
        number(&V::String(book_helper_string(value)?))
    } else {
        number(value)
    }
}
