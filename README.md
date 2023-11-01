# IRC-server
The overall design of the IRC sever is to have a single sever threads and multiple clients threads, 
  The sever thread is responsible for processing requests from client threads.
  The client threads are responsible for collecting requests from IRC clients and send to sever threads. 
  
  Although this design minimises the use of synchronisation primitives, (uses just one channel), it has just 1 sever threads, which maybe under heavy load when the number of client threads get large. 
