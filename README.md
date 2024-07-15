# Live Order Book

This project implements an extensible order book builder using an asynchronous and multithreaded runtime. Currently the Binance API is supported, and BTree and Fenwick Tree order books are implemented. The top bid and asks are periodically printed to the standard output. No API keys are currently required. Can be run with `cargo run`.
