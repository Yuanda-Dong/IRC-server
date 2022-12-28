use clap::Parser;
use iris_lib::{
    connect::{ConnectionError, ConnectionManager},
    plugin::{create_plugin, parse_plugin},
    types::{
        Channel, ErrorType, JoinMsg, JoinReply, Message, MyMessage, Nick, ParsedMessage, PartMsg,
        PartReply, PrivReply, QuitReply, Reply, Target, ThreadInfo, UnparsedMessage, WelcomeReply,
        SERVER_NAME,
    },
};
use log::{debug, error, info};
use std::{
    collections::{HashMap, HashSet},
    net::IpAddr,
    sync::mpsc::{self},
    thread::{self},
};

#[derive(Parser)]
struct Arguments {
    #[clap(default_value = "127.0.0.1")]
    ip_address: IpAddr,

    #[clap(default_value = "6991")]
    port: u16,
}

fn sever(arguments: Arguments) {
    info!(
        "Launching {} at {}:{}",
        SERVER_NAME, arguments.ip_address, arguments.port
    );
    let mut connection_manager = ConnectionManager::launch(arguments.ip_address, arguments.port);
    let (sender, receiver) = mpsc::channel::<(String, MyMessage)>(); //String for IP address + port
    {
        let sender = sender.clone(); // for create_plugin to work under borrow checker in the match statement
        thread::spawn(move || {
            let mut my_map: HashMap<String, ThreadInfo> = HashMap::new();
            let mut channels: HashMap<Channel, HashSet<String>> = HashMap::new();
            loop {
                debug!("User Info: {:?}", my_map);
                debug!("Channels: {:?}", channels);
                let message = receiver.recv().unwrap();
                let address = message.0;
                let request = message.1;
                match request {
                    MyMessage::Request(request) => {
                        let ThreadInfo {
                            conn_write,
                            nick,
                            full_name,
                        } = my_map.get_mut(&address).unwrap();
                        let request = UnparsedMessage {
                            sender_nick: match &nick {
                                Some(nick) => nick.clone(),
                                None => Nick("".to_string()),
                            },
                            message: &request,
                        };
                        if nick.is_none() && full_name.is_none() {
                            let parsed_message = ParsedMessage::try_from(request);
                            match parsed_message {
                                Ok(request) => match request.message {
                                    Message::Nick(name) => {
                                        if my_map
                                            .values()
                                            .into_iter()
                                            .any(|e| e.nick == Some(name.nick.clone()))
                                        {
                                            let conn_write =
                                                &mut my_map.get_mut(&address).unwrap().conn_write;
                                            conn_write
                                                .write_message(&format!(
                                                    "{}\n",
                                                    ErrorType::NickCollision
                                                ))
                                                .unwrap();
                                        } else {
                                            let nick = &mut my_map.get_mut(&address).unwrap().nick;
                                            *nick = Some(name.nick);
                                        }
                                    }
                                    Message::Quit(_) => {
                                        my_map.remove(&address).unwrap();
                                    }
                                    _ => {}
                                },
                                Err(e) => conn_write.write_message(&format!("{}\n", e)).unwrap(),
                            }
                        } else if nick.is_some() && full_name.is_none() {
                            let parsed_message = ParsedMessage::try_from(request);
                            match parsed_message {
                                Ok(request) => match request.message {
                                    Message::User(name) => {
                                        let reply = Reply::Welcome(WelcomeReply {
                                            target_nick: nick.as_ref().unwrap().clone(),
                                            message: format!(
                                                "Hi {}, welcome to IRC",
                                                name.real_name
                                            ),
                                        });
                                        conn_write.write_message(&reply.to_string()).unwrap();
                                        *full_name = Some(name.real_name);
                                    }
                                    Message::Quit(_) => {
                                        my_map.remove(&address).unwrap();
                                    }
                                    _ => {}
                                },
                                Err(e) => {
                                    conn_write.write_message(&format!("{}\n", e)).unwrap();
                                }
                            }
                        } else if nick.is_some() && full_name.is_some() {
                            let parsed_message = ParsedMessage::try_from(request);
                            match parsed_message {
                                Ok(request) => match request.message {
                                    Message::PrivMsg(msg) => match msg.target.clone() {
                                        Target::Channel(target) => {
                                            if !channels.contains_key(&target) {
                                                conn_write
                                                    .write_message(&format!(
                                                        "{}\n",
                                                        ErrorType::NoSuchChannel
                                                    ))
                                                    .unwrap();
                                            } else {
                                                let reply = Reply::PrivMsg(PrivReply {
                                                    message: msg,
                                                    sender_nick: nick.as_ref().unwrap().clone(),
                                                });
                                                let members = channels.get(&target).unwrap();
                                                for member in members {
                                                    let conn_write = &mut my_map
                                                        .get_mut(member)
                                                        .unwrap()
                                                        .conn_write;
                                                    conn_write
                                                        .write_message(&reply.to_string())
                                                        .unwrap();
                                                }
                                            }
                                        }
                                        Target::User(target) => {
                                            if my_map
                                                .values()
                                                .any(|e| e.nick == Some(target.clone()))
                                            {
                                                let ThreadInfo {
                                                    conn_write: _,
                                                    nick,
                                                    full_name: _,
                                                } = my_map.get_mut(&address).unwrap();
                                                let reply = Reply::PrivMsg(PrivReply {
                                                    message: msg,
                                                    sender_nick: nick.as_ref().unwrap().clone(),
                                                });
                                                let conn_write = &mut my_map
                                                    .values_mut()
                                                    .find(|e| e.nick == Some(target.clone()))
                                                    .unwrap()
                                                    .conn_write;
                                                conn_write
                                                    .write_message(&reply.to_string())
                                                    .unwrap();
                                            } else {
                                                let conn_write = &mut my_map
                                                    .get_mut(&address)
                                                    .unwrap()
                                                    .conn_write;
                                                conn_write
                                                    .write_message(&format!(
                                                        "{}\n",
                                                        ErrorType::NoSuchNick
                                                    ))
                                                    .unwrap()
                                            }
                                        }
                                    },
                                    Message::Ping(msg) => {
                                        let reply = Reply::Pong(msg);
                                        conn_write.write_message(&reply.to_string()).unwrap();
                                    }
                                    Message::Join(msg) => {
                                        let reply = Reply::Join(JoinReply {
                                            message: JoinMsg {
                                                channel: msg.channel.clone(),
                                            },
                                            sender_nick: nick.as_ref().unwrap().clone(),
                                        });
                                        channels
                                            .entry(msg.channel.clone())
                                            .and_modify(|c| {
                                                c.insert(address.clone());
                                            })
                                            .or_insert({
                                                let mut initial = HashSet::new();
                                                initial.insert(address.clone());
                                                initial
                                            });
                                        let members = channels.get(&msg.channel).unwrap();
                                        for member in members {
                                            let conn_write =
                                                &mut my_map.get_mut(member).unwrap().conn_write;
                                            conn_write.write_message(&reply.to_string()).unwrap();
                                        }
                                    }
                                    Message::Part(msg) => {
                                        if !channels.contains_key(&msg.channel) {
                                            conn_write
                                                .write_message(&format!(
                                                    "{}\n",
                                                    ErrorType::NoSuchChannel
                                                ))
                                                .unwrap();
                                        } else if channels
                                            .get_mut(&msg.channel)
                                            .unwrap()
                                            .remove(&address)
                                        {
                                            let reply = Reply::Part(PartReply {
                                                message: PartMsg {
                                                    channel: msg.channel.clone(),
                                                },
                                                sender_nick: nick.as_ref().unwrap().clone(),
                                            });
                                            let members = channels.get(&msg.channel).unwrap();
                                            for member in members {
                                                let conn_write =
                                                    &mut my_map.get_mut(member).unwrap().conn_write;
                                                conn_write
                                                    .write_message(&reply.to_string())
                                                    .unwrap();
                                            }
                                        }
                                    }
                                    Message::Quit(msg) => {
                                        let reply = Reply::Quit(QuitReply {
                                            message: msg,
                                            sender_nick: nick.as_ref().unwrap().clone(),
                                        });
                                        for (_, members) in channels
                                            .iter_mut()
                                            .filter(|(_, v)| v.contains(&address))
                                        {
                                            members.remove(&address);
                                            for member in members.iter() {
                                                let conn_write =
                                                    &mut my_map.get_mut(member).unwrap().conn_write;
                                                conn_write
                                                    .write_message(&reply.to_string())
                                                    .unwrap();
                                            }
                                        }
                                        my_map.remove(&address).unwrap();
                                    }
                                    _ => {}
                                },
                                Err(e) => {
                                    conn_write.write_message(&format!("{}\n", e)).unwrap();
                                }
                            }
                        }
                    }
                    MyMessage::Est(ip, conn_write) => {
                        my_map.insert(
                            ip,
                            ThreadInfo {
                                conn_write,
                                nick: None,
                                full_name: None,
                            },
                        );
                    }
                    MyMessage::Plugin(plugin) => {
                        let plugin_function = create_plugin(
                            plugin,
                            address,
                            sender.clone(),
                            &mut channels,
                            &mut my_map,
                        );
                        plugin_function();
                    }
                }
            }
        });
    }
    loop {
        let (mut conn_read, conn_write) = connection_manager.accept_new_connection();
        info!("New connection from {}", conn_read.id());
        let sender = sender.clone();
        sender
            .send((conn_read.id(), MyMessage::Est(conn_read.id(), conn_write)))
            .unwrap();
        {
            thread::spawn(move || loop {
                let message = match conn_read.read_message() {
                    Ok(message) => message,
                    Err(ConnectionError::ConnectionLost | ConnectionError::ConnectionClosed) => {
                        sender
                            .send((conn_read.id(), MyMessage::Request("QUIT".to_string())))
                            .unwrap();
                        debug!("Lost connection from {}.", conn_read.id());
                        break;
                    }
                    Err(e) => {
                        error!("{}", e);
                        error!("Invalid message received... ignoring message.");
                        continue;
                    }
                };
                if message == "QUIT" || message.starts_with("QUIT ") {
                    sender
                        .send((conn_read.id(), MyMessage::Request(message)))
                        .unwrap();
                    break;
                } else if message.starts_with("PLUGIN") {
                    let rest: String = message.split("PLUGIN ").skip(1).collect();
                    if let Some(plugin) = parse_plugin(rest) {
                        sender
                            .send((conn_read.id(), MyMessage::Plugin(plugin)))
                            .unwrap();
                    }
                } else {
                    sender
                        .send((conn_read.id(), MyMessage::Request(message)))
                        .unwrap();
                }
            });
        }
    }
}

fn main() {
    env_logger::init();
    let arguments = Arguments::parse();
    sever(arguments);
}

#[cfg(test)]
mod tests {
    use crate::{sever, Arguments};
    use bufstream::BufStream;
    use serial_test::serial;
    use std::{
        io::{BufRead, Write},
        net::{IpAddr, Ipv4Addr, TcpStream},
        thread::{self, sleep},
        time::Duration,
    };
    fn spawn() {
        let arguments = Arguments {
            ip_address: IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)),
            port: 6991,
        };
        {
            thread::spawn(move || sever(arguments));
        }
        sleep(Duration::from_millis(200));
    }

    fn setup() -> (TcpStream, BufStream<TcpStream>) {
        let ip_address = IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1));
        let port = 6991;
        let stream_read = TcpStream::connect((ip_address, 6991))
            .expect(&format!("failed to connect to {ip_address}:{port}"));
        let stream_write = stream_read.try_clone().expect("failed to clone connection");
        let stream_read = BufStream::new(stream_read);
        (stream_write, stream_read)
    }

    fn command(stream_write: &mut TcpStream, command: &str) {
        stream_write
            .write_all(format!("{}\r\n", command).as_bytes())
            .and_then(|_| stream_write.flush())
            .unwrap();
    }

    fn receive(stream_read: &mut BufStream<TcpStream>) -> String {
        let mut buf = String::new();
        stream_read.read_line(&mut buf).unwrap();
        buf
    }

    fn register_user(
        nick: &str,
        stream_write: &mut TcpStream,
        stream_read: &mut BufStream<TcpStream>,
    ) {
        command(stream_write, &format!("NICK {}", nick));
        command(
            stream_write,
            &format!("USER ignored ignored ignored {}", nick),
        );
        assert_eq!(
            &format!(":iris-server 001 {} :Hi {}, welcome to IRC", nick, nick),
            receive(stream_read).trim()
        );
    }

    #[test]
    #[serial]
    fn single_client_registration() {
        spawn();
        let (mut stream_write, mut stream_read) = setup();
        register_user("nick", &mut stream_write, &mut stream_read);
    }

    #[test]
    #[serial]
    fn single_client_msg_self() {
        spawn();
        let (mut stream_write, mut stream_read) = setup();
        register_user("nick", &mut stream_write, &mut stream_read);
        command(&mut stream_write, "PRIVMSG nick :How are you?");
        assert_eq!(
            ":nick PRIVMSG nick :How are you?",
            receive(&mut stream_read).trim()
        );
    }

    #[test]
    #[serial]
    fn single_client_ping() {
        spawn();
        let (mut stream_write, mut stream_read) = setup();
        register_user("nick", &mut stream_write, &mut stream_read);
        command(&mut stream_write, "PING :Hello, world!");
        assert_eq!("PONG :Hello, world!", receive(&mut stream_read).trim());
    }

    #[test]
    #[serial]
    fn single_client_quit() {
        spawn();
        let (mut stream_write, mut stream_read) = setup();
        register_user("nick", &mut stream_write, &mut stream_read);
        command(&mut stream_write, "QUIT");
    }

    #[test]
    #[serial]
    fn single_client_channel1() {
        spawn();
        let (mut stream_write, mut stream_read) = setup();
        register_user("nick", &mut stream_write, &mut stream_read);
        command(&mut stream_write, "JOIN #haku");
        assert_eq!(":nick JOIN #haku", receive(&mut stream_read).trim());
        command(&mut stream_write, "PART #haku");
    }
    #[test]
    #[serial]
    fn single_client_channel2() {
        spawn();
        let (mut stream_write, mut stream_read) = setup();
        register_user("nick", &mut stream_write, &mut stream_read);
        command(&mut stream_write, "JOIN #haku");
        assert_eq!(":nick JOIN #haku", receive(&mut stream_read).trim());
        command(&mut stream_write, "PRIVMSG #haku Hello,world!");
        assert_eq!(
            ":nick PRIVMSG #haku :Hello,world!",
            receive(&mut stream_read).trim()
        );
    }

    #[test]
    #[serial]
    fn multi_client_registration() {
        spawn();
        let (mut stream_write1, mut stream_read1) = setup();
        register_user("nick1", &mut stream_write1, &mut stream_read1);
        let (mut stream_write2, mut stream_read2) = setup();
        register_user("nick2", &mut stream_write2, &mut stream_read2);
    }

    #[test]
    #[serial]
    fn multi_client_msg() {
        spawn();
        let (mut stream_write1, mut stream_read1) = setup();
        register_user("nick1", &mut stream_write1, &mut stream_read1);
        let (mut stream_write2, mut stream_read2) = setup();
        register_user("nick2", &mut stream_write2, &mut stream_read2);
        command(&mut stream_write1, "PRIVMSG nick2 :How are you?");
        assert_eq!(
            ":nick1 PRIVMSG nick2 :How are you?",
            receive(&mut stream_read2).trim()
        );
    }

    #[test]
    #[serial]
    fn multi_client_channel() {
        spawn();
        let (mut stream_write1, mut stream_read1) = setup();
        register_user("nick1", &mut stream_write1, &mut stream_read1);
        let (mut stream_write2, mut stream_read2) = setup();
        register_user("nick2", &mut stream_write2, &mut stream_read2);

        command(&mut stream_write1, "JOIN #haku");
        assert_eq!(":nick1 JOIN #haku", receive(&mut stream_read1).trim());

        command(&mut stream_write2, "JOIN #haku");
        assert_eq!(":nick2 JOIN #haku", receive(&mut stream_read1).trim());
        assert_eq!(":nick2 JOIN #haku", receive(&mut stream_read2).trim());

        command(&mut stream_write1, "PRIVMSG #haku Hello,world!");
        assert_eq!(
            ":nick1 PRIVMSG #haku :Hello,world!",
            receive(&mut stream_read1).trim()
        );
        assert_eq!(
            ":nick1 PRIVMSG #haku :Hello,world!",
            receive(&mut stream_read2).trim()
        );
    }

    #[test]
    #[serial]
    fn multi_client_quit() {
        spawn();
        let (mut stream_write1, mut stream_read1) = setup();
        register_user("nick1", &mut stream_write1, &mut stream_read1);
        let (mut stream_write2, mut stream_read2) = setup();
        register_user("nick2", &mut stream_write2, &mut stream_read2);

        command(&mut stream_write1, "JOIN #haku");
        assert_eq!(":nick1 JOIN #haku", receive(&mut stream_read1).trim());

        command(&mut stream_write2, "JOIN #haku");
        assert_eq!(":nick2 JOIN #haku", receive(&mut stream_read1).trim());
        assert_eq!(":nick2 JOIN #haku", receive(&mut stream_read2).trim());

        command(&mut stream_write1, "QUIT");
        assert_eq!(":nick1 QUIT :nick1", receive(&mut stream_read2).trim());
    }
}
