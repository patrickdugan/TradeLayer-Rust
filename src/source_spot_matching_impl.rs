// Included in the future literal Core; no balances or separate book authority.
fn spot_number(value: &V) -> R<f64> {
    if matches!(value, V::Object(_) | V::Array(_)) {
        source_channel_string(value)?;
    }
    number(value)
}
fn spot_strict(left: &V, right: &V) -> bool {
    match (left, right) {
        (V::Undefined, V::Undefined) | (V::Null, V::Null) => true,
        (V::Bool(a), V::Bool(b)) => a == b,
        (V::String(a), V::String(b)) => a == b,
        (V::Number(a), V::Number(b)) => f64::from_bits(*a) == f64::from_bits(*b),
        _ => false,
    }
}
fn spot_parse_float(value: &V) -> R<f64> {
    let value = source_channel_string(value)?;
    let value = value.trim_start_matches(funding_js_space);
    let bytes = value.as_bytes();
    let mut i = usize::from(bytes.first().is_some_and(|b| matches!(b, b'+' | b'-')));
    if value.get(i..).is_some_and(|s| s.starts_with("Infinity")) {
        return Ok(if bytes.first() == Some(&b'-') {
            f64::NEG_INFINITY
        } else {
            f64::INFINITY
        });
    }
    let mut digits = 0;
    while bytes.get(i).is_some_and(u8::is_ascii_digit) {
        i += 1;
        digits += 1;
    }
    if bytes.get(i) == Some(&b'.') {
        i += 1;
        while bytes.get(i).is_some_and(u8::is_ascii_digit) {
            i += 1;
            digits += 1;
        }
    }
    if digits == 0 {
        return Ok(f64::NAN);
    }
    if bytes.get(i).is_some_and(|b| matches!(b, b'e' | b'E')) {
        let start = i;
        i += 1;
        if bytes.get(i).is_some_and(|b| matches!(b, b'+' | b'-')) {
            i += 1;
        }
        let exponent = i;
        while bytes.get(i).is_some_and(u8::is_ascii_digit) {
            i += 1;
        }
        if i == exponent {
            i = start;
        }
    }
    Ok(value[..i].parse().unwrap_or(f64::NAN))
}
fn spot_compare(left: &V, right: &V) -> R<Option<core::cmp::Ordering>> {
    if matches!(left, V::Object(_) | V::Array(_)) {
        source_channel_string(left)?;
    }
    if matches!(right, V::Object(_) | V::Array(_)) {
        source_channel_string(right)?;
    }
    crate::oracle_source_owner::source_comparison_unverified(left, right).map_err(Fault::Local)
}
fn spot_loose_bool(value: &V, expected: bool) -> R<bool> {
    if matches!(value, V::Undefined | V::Null) {
        return Ok(false);
    }
    if matches!(
        value,
        V::Object(_) | V::Array(_) | V::BigNumber(_) | V::Promise
    ) {
        return Err(SourceRuleError::UnsupportedNumericDomain.into());
    }
    Ok(spot_number(value)? == if expected { 1. } else { 0. })
}
fn spot_key(left: &V, right: &V) -> R<V> {
    let (a, b) = if spot_compare(left, right)? == Some(core::cmp::Ordering::Less) {
        (left, right)
    } else {
        (right, left)
    };
    Ok(s(format!(
        "{}-{}",
        source_channel_string(a)?,
        source_channel_string(b)?
    )))
}
fn spot_plus(left: &V, right: &V) -> R<V> {
    let primitive = |value: &V| -> R<V> {
        Ok(
            if matches!(
                value,
                V::Array(_) | V::Object(_) | V::BigNumber(_) | V::Promise
            ) {
                s(source_channel_string(value)?)
            } else {
                value.clone()
            },
        )
    };
    let left = primitive(left)?;
    let right = primitive(right)?;
    if matches!(left, V::String(_)) || matches!(right, V::String(_)) {
        Ok(s(format!(
            "{}{}",
            source_channel_string(&left)?,
            source_channel_string(&right)?
        )))
    } else {
        Ok(n(spot_number(&left)? + spot_number(&right)?))
    }
}
fn spot_price(offered: &V, expected: &V) -> R<V> {
    Ok(n(FundingBn::new(offered)?
        .op(FundingBn::new(expected)?, 3)?
        .q(8, FundingRounding::HalfUp)?
        .number()))
}
fn spot_min(left: FundingBn, right: FundingBn) -> R<FundingBn> {
    Ok(match left.compare(&right) {
        None => FundingBn::new(&n(f64::NAN))?,
        Some(core::cmp::Ordering::Greater) => right,
        Some(core::cmp::Ordering::Equal)
            if left.number() == 0. && right.number() == 0. && right.number().is_sign_negative() =>
        {
            right
        }
        _ => left,
    })
}
fn spot_rows(book: &V, key: &str) -> Vec<V> {
    if let V::Array(rows) = book.get(key) {
        rows
    } else {
        vec![]
    }
}
fn spot_set_origin(book: &mut V, side: &str, index: usize, value: V) -> R<()> {
    let mut rows = array(&book.get(side))?;
    *rows.get_mut(index).ok_or(SourceRuleError::InvalidInput)? = value;
    set(book, side, V::Array(rows))
}
fn spot_sort(rows: &[V], buy: bool) -> R<Vec<usize>> {
    if rows.len() > 1024 {
        return Err(SourceRuleError::Capacity.into());
    }
    let mut ids: Vec<usize> = (0..rows.len()).collect();
    let cmp = |a: usize, b: usize| -> R<core::cmp::Ordering> {
        let left = FundingBn::new(&funding_prop(&rows[a], "price")?)?;
        let right = FundingBn::new(&funding_prop(&rows[b], "price")?)?;
        let price = if buy {
            right.compare(&left)
        } else {
            left.compare(&right)
        };
        if let Some(order) = price {
            if order != core::cmp::Ordering::Equal {
                return Ok(order);
            }
        }
        let diff = spot_number(&funding_prop(&rows[a], "blockTime")?)?
            - spot_number(&funding_prop(&rows[b], "blockTime")?)?;
        Ok(if diff < 0. {
            core::cmp::Ordering::Less
        } else if diff > 0. {
            core::cmp::Ordering::Greater
        } else {
            core::cmp::Ordering::Equal
        })
    };
    // Stable price/time ordering over the admitted ordinary source domain.
    for i in 1..ids.len() {
        let mut j = i;
        while j > 0 && cmp(ids[j], ids[j - 1])? == core::cmp::Ordering::Less {
            ids.swap(j, j - 1);
            j -= 1;
        }
    }
    // Nontransitive caller sort domains are explicit local admission failures.
    for a in 0..ids.len() {
        for b in a + 1..ids.len() {
            if cmp(ids[a], ids[b])? == core::cmp::Ordering::Greater {
                return Err(SourceRuleError::UnsupportedNumericDomain.into());
            }
        }
    }
    Ok(ids)
}
fn spot_match_token_book(book: &mut V, tick_size: &V) -> R<V> {
    if !matches!(
        book,
        V::Object(_) | V::Array(_) | V::BigNumber(_) | V::Promise
    ) {
        return Ok(obj(&[
            (
                "orderBook",
                obj(&[("buy", V::Array(vec![])), ("sell", V::Array(vec![]))]),
            ),
            ("matches", V::Array(vec![])),
        ]));
    }
    let mut buys = spot_rows(book, "buy");
    let mut sells = spot_rows(book, "sell");
    let mut buy_ids = spot_sort(&buys, true)?;
    let mut sell_ids = spot_sort(&sells, false)?;
    let mut matches = vec![];
    for _ in 0..1000 {
        let (Some(&bi), Some(&si)) = (buy_ids.first(), sell_ids.first()) else {
            break;
        };
        let mut buy = buys[bi].clone();
        let mut sell = sells[si].clone();
        if !spot_strict(
            &funding_prop(&sell, "offeredPropertyId")?,
            &funding_prop(&buy, "desiredPropertyId")?,
        ) || !spot_strict(
            &funding_prop(&sell, "desiredPropertyId")?,
            &funding_prop(&buy, "offeredPropertyId")?,
        ) {
            break;
        }
        let (mut bump, mut post) = (false, false);
        set(&mut sell, "maker", V::Bool(false))?;
        set(&mut buy, "maker", V::Bool(false))?;
        let sell_time = funding_prop(&sell, "blockTime")?;
        let buy_time = funding_prop(&buy, "blockTime")?;
        let trade_price = if spot_strict(&sell_time, &buy_time) {
            let mut price = funding_prop(&buy, "price")?;
            if truth(&funding_prop(&sell, "post")?) {
                price = funding_prop(&sell, "price")?;
                post = true;
                set(&mut sell, "maker", V::Bool(true))?;
            } else if truth(&funding_prop(&buy, "post")?) {
                post = true;
                set(&mut buy, "maker", V::Bool(true))?;
            }
            set(&mut sell, "flat", V::Bool(true))?;
            price
        } else {
            let earlier = spot_compare(&sell_time, &buy_time)? == Some(core::cmp::Ordering::Less);
            let later = spot_compare(&sell_time, &buy_time)? == Some(core::cmp::Ordering::Greater);
            bump = earlier && truth(&funding_prop(&buy, "post")?)
                || later && truth(&funding_prop(&sell, "post")?);
            if earlier && !bump {
                set(&mut sell, "maker", V::Bool(true))?;
            } else if later && !bump {
                set(&mut buy, "maker", V::Bool(true))?;
            }
            if earlier {
                funding_prop(&sell, "price")?
            } else {
                funding_prop(&buy, "price")?
            }
        };
        // The shallow array copies retain the original order objects.
        buys[bi] = buy.clone();
        sells[si] = sell.clone();
        spot_set_origin(book, "buy", bi, buy.clone())?;
        spot_set_origin(book, "sell", si, sell.clone())?;
        if spot_strict(
            &funding_prop(&sell, "sender")?,
            &funding_prop(&buy, "sender")?,
        ) {
            if truth(&funding_prop(&sell, "maker")?) {
                sell_ids.remove(0);
            } else if truth(&funding_prop(&buy, "maker")?) {
                buy_ids.remove(0);
            }
            continue;
        }
        let buy_price = FundingBn::new(&funding_prop(&buy, "price")?)?;
        let sell_price = FundingBn::new(&funding_prop(&sell, "price")?)?;
        if !matches!(
            buy_price.compare(&sell_price),
            Some(core::cmp::Ordering::Greater | core::cmp::Ordering::Equal)
        ) {
            break;
        }
        let sell_offered = FundingBn::new(&funding_prop(&sell, "amountOffered")?)?;
        let sell_expected = FundingBn::new(&funding_prop(&sell, "amountExpected")?)?;
        let buy_offered = FundingBn::new(&funding_prop(&buy, "amountOffered")?)?;
        let buy_expected = FundingBn::new(&funding_prop(&buy, "amountExpected")?)?;
        let amount_a = spot_min(sell_offered.clone(), buy_expected.clone())?;
        let amount_b = amount_a.clone().op(FundingBn::new(&trade_price)?, 2)?;
        if !bump {
            set(
                &mut sell,
                "amountOffered",
                n(sell_offered.op(amount_a.clone(), 1)?.number()),
            )?;
            set(
                &mut buy,
                "amountOffered",
                n(buy_offered.op(amount_b.clone(), 1)?.number()),
            )?;
            set(
                &mut sell,
                "amountExpected",
                n(sell_expected.op(amount_b.clone(), 1)?.number()),
            )?;
            set(
                &mut buy,
                "amountExpected",
                n(buy_expected.op(amount_a.clone(), 1)?.number()),
            )?;
            let mut sold = sell.clone();
            set(&mut sold, "amountOffered", n(amount_a.number()))?;
            let mut bought = buy.clone();
            set(&mut bought, "amountExpected", n(amount_b.number()))?;
            matches.push(obj(&[
                ("sellOrder", sold),
                ("buyOrder", bought),
                ("amountOfTokenA", n(amount_a.number())),
                ("amountOfTokenB", n(amount_b.number())),
                ("tradePrice", trade_price),
                ("post", V::Bool(post)),
                ("bumpTrade", V::Bool(bump)),
            ]));
            if spot_strict(&sell.get("amountOffered"), &n(0.)) {
                sell_ids.remove(0);
            }
            if spot_strict(&buy.get("amountExpected"), &n(0.)) {
                buy_ids.remove(0);
            }
        } else {
            if truth(&funding_prop(&buy, "post")?) {
                set(
                    &mut buy,
                    "price",
                    n(spot_number(&funding_prop(&sell, "price")?)? - spot_number(tick_size)?),
                )?;
            }
            if truth(&funding_prop(&sell, "post")?) {
                set(
                    &mut sell,
                    "price",
                    spot_plus(&funding_prop(&buy, "price")?, tick_size)?,
                )?;
            }
        }
        buys[bi] = buy.clone();
        sells[si] = sell.clone();
        spot_set_origin(book, "buy", bi, buy)?;
        spot_set_origin(book, "sell", si, sell)?;
    }
    Ok(obj(&[
        (
            "orderBook",
            obj(&[
                (
                    "buy",
                    V::Array(buy_ids.into_iter().map(|i| buys[i].clone()).collect()),
                ),
                (
                    "sell",
                    V::Array(sell_ids.into_iter().map(|i| sells[i].clone()).collect()),
                ),
            ]),
        ),
        ("matches", V::Array(matches)),
    ]))
}
