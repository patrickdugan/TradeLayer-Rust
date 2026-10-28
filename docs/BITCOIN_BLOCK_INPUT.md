# Block-bound selected-input satisfaction

`check_bitcoin_block_input` composes complete-block BIP141 byte binding with
the supported Bitcoin payment/channel input verifier. Its only spending input
is a transaction/input position in the supplied raw block. It accepts no
detached spending transaction, signature verdict, public-key claim or amount.
The underlying checker reads the actual funding transaction and output.

The immutable result borrows the block and funding bytes. Its input receipt
refers to the exact transaction slice used by the block relation. This prevents
pairing a valid detached signature check with different witness bytes that share
the same ordinary TXID. Recommitting an invalid witness can satisfy BIP141, but
the composed check still rejects its actual invalid signature.

Expected header, parent, rule context and script flags are independent caller
expectations. Their equality is not chain authentication. Only the selected
input is checked. Other inputs, funding inclusion/unspentness, finality, value
conservation and remaining chain consensus are unproven. NONE/SINGLE and the
legacy SINGLE bug retain their actual reduced commitments. The existing three
authority requests remain permanent errors; no economic admission path is added.

Focused tests compose all six genuine-Core-checked input templates with
synthetic structural blocks, verify borrowed transaction identity, reject both
old-commitment witness mutation and recommitted invalid witness, and exercise
substituted context, flags, funding and positions. Weak hash-mode cases retain
their original results and all authority refusals. The synthetic blocks are
not presented as historical, mined or consensus-valid blocks.
