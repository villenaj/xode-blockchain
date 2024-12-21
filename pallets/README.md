# Pallets

### Pallet Guide
https://paritytech.github.io/polkadot-sdk/master/polkadot_sdk_docs/guides/your_first_pallet/index.html
https://polkadot.study/tutorials/substrate-in-bits/

### Polkadot SDK Stable 2409
https://github.com/paritytech/polkadot-sdk/tree/stable2409

### Collator Selection
https://github.com/paritytech/polkadot-sdk/blob/stable2409/cumulus/pallets/collator-selection/src/lib.rs#L841

### OnUnbalanced
https://github.com/paritytech/substrate/blob/master/bin/node/runtime/src/lib.rs

### Generate Weights
`cargo run --release \
           --features runtime-benchmarks \
           --benchmark pallet \
           --execution=wasm --wasm-execution=compiled \
           --heap-pages=4096 \
           --pallet pallet-xode-staking \
           --extrinsic "*" \
           --steps 50 \
           --repeat 20 \
           --output ./pallets/staking/src/weights.rs \
           --template ./pallets/staking/src/weight-template.hbs`