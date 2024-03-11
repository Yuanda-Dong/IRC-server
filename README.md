# IRC-server
The overall design of the IRC sever is to have a single sever thread and multiple clients threads, 
  The sever thread is responsible for processing requests from client threads.
  The client threads are responsible for collecting requests from IRC clients and send to sever threads. 
  
  Although this design minimises the use of synchronisation primitives, (uses just one channel), it has just 1 sever threads, which maybe under heavy load when the number of client threads get large. 
  
## Quick start 
- The server can be started with the command `cargo run 127.0.0.1 6991`.
- A compliant IRC client such as `sic` can be used with the server.
