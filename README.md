# Live Order Book

This project implements a realtime orderbook builder in Rust. The aim of this project is to showcase various multithreading/asynchronous programming techniques.

For the time being, this project implements connections to one exchange, Binance, and implements a single basic order book using a BTree.

The project is designed to be extensible, and different exchange connectors and orderbook structurs can be implemented and slotted in using the defined traits.


## Running the program

To run the program, `cd` into the root directory and run `cargo build`, and then run the resulting executable.

To configure the program, look at `config.toml`. For the time being the only attributes of the program that can be configured are the Binance asset pairs. Asset pairs can be added or removed from the respective field in the config.

## More about the project

The main constructs in this project are connectors and order books. Connectors facilitate and govern the connection to an exchange, whilst order books represent the underlying datastructure of the book, and expose an interface to modify the book. Connectors are also, in this version of the project, responsible for for maintaining a consistent orderbook. Connectors must facilitate connection, reconnection and the handling of updates.

At the start of execution, the configuration file is read and parsed. Based on this, the necessary `tokio` tasks are spawned, which create and initialise a connector and orderbook pair, and begin listening to the exchange. Each task is being run asynchronously and concurrently. 

With respect to the Binance connector, when the connector first starts - that is, when it first connects - it will spawn a task that listens to the Binance Websocket stream, where incoming events are buffered into a transaction channel. Whilst that task is running, a synchronous update of the entire orderbook using a snapshot of the Binance book is committed. Once that snapshot update is successful, a processing task picks up any buffered events on the transaction channel and begins acting on them. Events up to the last receieved event in the snapshot are ignored, and all subsequent events are processed as updates to the order book. 

Errors are through the transaction channel, and when an error is found, the channel is closed and the connector exits - but will continue retrying the connection. On the retry, the exact same process is committed.

This continuous retry and snapshotting is what gives the orderbook builder resiliency. If we have dropped a connection, then we will start listening again, retrieve a snapshot, bring the book up to order, and resume processing events. 


