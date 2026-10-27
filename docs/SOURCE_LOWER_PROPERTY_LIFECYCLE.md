# Lower Property lifecycle (V7)

Status: the actual Rust 1.90 Core passes all three focused Property tests with archived BigNumber 9.3.1 and lockfile 9.1.2, including 109 strictly compared unchanged-source calls and one separately counted local JSON-domain refusal. The combined focused run passed 21 tests across seven targets; the locked pass passed eight tests across the three new targets. The independently captured V6 release fixture and inherited proof, procedural, rewards and tail corpora also pass under V7. This is bounded method-graph evidence, not a whole lower-transaction graph claim.

## Source graph and authority

The explicit `LowerLifecycleV7` policy adds raw types 1, 11 and 12 to the existing actual `Types.decodePayload -> Validity -> Logic.typeSwitch` path. Direct methods are `Logic.tokenIssue`, `Logic.grantManagedToken`, `Logic.redeemManagedToken`; reached new Property methods are `createToken`, `getNextPropertyId`, `addProperty`, `isManagedAndAdmin` and `doesTickerExist`. The existing Core Property singleton/cache, Property DB collection, Tally owner and Procedural methods remain the only state authorities. No parallel Property registry, balance ledger, caller-supplied validity, reward, or financial result is introduced.

The pinned source commit is `dca0888b827c5bc28fa9189bc9e1d507f5c5670c`. Full LF-normalized files were independently read with `git show` and compared with the repository archives: `src/property.js` SHA256 `ef0ee7ba5c362227a8bfd44192e351c107871a93a3dc93b04ce6d1a5ebfc70b1`, `src/logic.js` `f09d476e581d38cf7278f1424aadba7266f25e0fbc23763ceba4afa79c5b3e1d`, `src/procedural.js` `0a89e5447a07d592a25032d55ced478b01797cda96da055c888a0da6664f062e`.

Inspected methods and archive lines: Logic `typeSwitch` 147, `tokenIssue` 323, `grantManagedToken` 979, `redeemManagedToken` 993; Property `getNextPropertyId` 61, `createToken` 70, `addProperty` 88, `isManagedAndAdmin` 186, `doesTickerExist` 283, `grantTokens` 316, `redeemTokens` 337. Types/decoder/Validity are the existing complete archived modules, not replacement validation functions.

## Executed source evidence

The first-ten proof contains one reset and ten actual raw calls. Fixed issuance creates property 8, managed issuance creates property 9, two grants change managed supply from 1000 to 1002 to 1005, and admin redemption changes it to 1003. Reference-address and sender fallback both execute. Fixed-property grant, outsider redemption, duplicate ticker and zero grant are actually rejected; procedural issuance creates property 10. The source duplicate-ticker reason literally starts `undefinedTicker already exists; `.

The expanded corpus currently has 137 observations: 26 resets, one external persisted-row replacement, and 110 genuine source calls. Of the calls, 109 have passed strict Rust/source comparison and one malformed-JSON call is explicitly a local unsupported-domain test, not parity. Observations include complete source outcomes, imported/persisted rows, actual Property/Tally caches and flags, original transaction IDs, complete ordered DB/environment calls, the declared observed source-method calls, and unhandled rejections. Each comparable Rust call round-trips the full actual replay checkpoint against its independently expected head.

The corpus executes actual Procedural template and contract writes, OPEN grant and SETTLED non-admin redemption; default and alternate CommonJS entry profiles; source-generated multi-whitelist payloads; failed Property/Tally factories, reads, updates and delta inserts with retries; stale/empty/malformed Property rows; returned native Error objects; NaN property-ID allocation; source quantization and caught negative/NaN initial-balance errors. Only storage, activation, UUID, clock, console and external imports are adapter boundaries. The console adapter records arguments and returns undefined, matching the ordinary Node host; the older reused driver's accidental logs.push-length return is not allowed to become addProperty's result. No method body of the financial, validation, Property or Procedural source is changed. Transparent wrappers record original arguments and return the original method result unchanged.

## Retained source behavior

* `tokenIssue` awaits creation and Tally credit, catches their exceptions, and returns the exception message. Raw `typeSwitch` discards that returned message. Property metadata can persist before a later Tally failure.
* The duplicate check runs against the current Property cache before `getNextPropertyId` loads the DB. A cached matching ticker returns a native Error object rather than throwing; `tokenIssue` ignores the Tally invalid-property Error and interpolates the Error as its new property ID. Failed Property-save retries reach this path. A cached key equal to the ticker instead throws `TypeError: ticker is not a constructor`.
* Property type lookup uses JavaScript ToPropertyKey coercion: an own non-callable `toString` throws the original TypeError before writes; `['Fixed']` really creates the Fixed property.
* `getNextPropertyId` uses literal `Math.max` over all source keys. A nonnumeric key produces NaN; it is not skipped or repaired. JSON persistence converts a NaN map key to null while the live cache retains NaN.
* Property supply is quantized to eight decimals with HALF_UP. Tally receives the original grant amount or JavaScript unary-minus redemption amount. Property supply/cache changes precede the actual Tally call, and the final Property save occurs afterward.
* The raw1 encoder joins whitelist values with commas, but the decoder splits the whitelist field with semicolons. Multiple encoded whitelist entries shift later fields; that source rejection is preserved.
* Logic recognizes procedural properties using `Number(propertyData?.type) === 7`, whereas Validity also accepts the string `Procedural`. Issuance/redemption contexts are checked by the actual shared Procedural owner at each actual source call.
* The native Base58 decoder adds an extra zero byte to common Bitcoin addresses beginning with `1`. Ordinary success scenes use genuine Litecoin Base58 inputs, independently produced by bitcoinjs-lib with version byte 48. This bypasses no source checks. RPC address-validation fallback remains an explicit missing external capability, not a supplied validity verdict.

## Explicit limits and compatibility

V1-V6 policy routing, state projections, replay framing and admissions remain unchanged. The new include requires only `lower_lifecycle_enabled()` and the existing Core fields; the work-only `prepare_runtime(alreadyV7)` fragment supplies its routes/raw bridge/include. Policy enum/bootstrap/TLCR7 integration is coordinated separately.

Native returned Error values are observed through their name/message diagnostic projection, consistent with existing source owners. Only the actual result of this `createToken` call is treated as such by `tokenIssue`; arbitrary caller objects are not reclassified. Native Error prototype, property descriptors and object identity are outside this SourceValue experiment.

Composite actor/Map-key identity and inherited Property type-map functions/prototypes are refused locally. They are not converted into successful source results. Existing SourceValue and Core table/cache/node/byte/depth limits remain in force; local admission/capacity failures roll back the entire candidate Core, including effects. Native source exceptions retain their actual write prefixes.

The existing generic Core JSON catch marker is insufficient to expose a native uncaught JSON SyntaxError with engine-specific text. `doesTickerExist` therefore refuses that malformed-JSON domain locally; the genuine Node SyntaxError is retained as separately counted evidence. Caught JSON failures in Property load/admin/data reads retain the actual fallback/cache behavior. There is no whole lower-type 0-15, daemon-startup, L1 authentication, scanner composition, or RPC availability claim.

## Validation and handoff

Run `cargo test --frozen --test source_lower_property_lifecycle` from a clean checkout with the existing genuine source dependencies provisioned through `TRADELAYER_JS_ROOT`. The test embeds the complete reference driver and uses `CARGO_MANIFEST_DIR` for the pinned archives. It executes the actual Node modules at test time, compares actual mutable Core outputs/state/effects, and independently replays every comparable prefix. It does not trust stored poststate as execution proof. Set `TL_SOURCE_BN_VERSION=9.1.2` for the lockfile-constructor pass; the default is archived 9.3.1.

The work-only first-ten/expanded generator and JSONL files are audit scratch, not required inputs to the clean-checkout Rust target. They regenerated identically under both versions. The two native Error projection/coercion scenes and the console return boundary are checked by the embedded corpus, including the reviewer-proven own-toString first failure.

The combined Rust 1.90 command used `--no-fail-fast` for `source_lower_oracle_lifecycle`, `source_lower_property_lifecycle`, `source_lower_lifecycle_versions`, `source_proof_lifecycle`, `source_procedural_bitvm`, `source_liquidity_rewards` and `source_tail_lifecycle`: 21 tests passed. The locked lower Oracle/Property/version rerun passed all eight tests. Full-suite, lint and release gates belong to the parent integration batch; these focused results do not claim those later gates.
