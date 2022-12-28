#![warn(missing_docs)]
//! # Plugin Lib
//! This is a rust library that can be used for creating plugins.
//!  
//! You can learn about how to create a plugin by looking at how Listing plugin is made here.
//!
//! In order to create a plugin, you need to
//! - add a variant to Plugin enum
//! - match the variant in create_plugin function
//! - add parsing (string -> your plugin) to parse_plugin function
//! - create the actual function to be ran in the sever thread  
//! Please check out how Listing plugin is implemented.
use std::{
    collections::{HashMap, HashSet},
    sync::mpsc::Sender,
    thread::{self, sleep},
    time::Duration,
};

use crate::types::{Channel, MyMessage, ThreadInfo};

/// Plugin enum can be used to create plugin functions that are ran in the sever thread, by calling create_plugin
pub enum Plugin {
    /// A remainder plugin
    /// Duration: in seconds
    ///
    /// String: nickname of receiver
    ///
    /// String: message to be sent
    Remainder(Duration, String, String),
    /// A plugin for Listing all available channels by name
    ListingChannels,
}
/// create_plugin creates a function from a plugin enum, the function will be ran in the sever thread
/// In addition to the Plugin enum and its associated parameters, we provide more parameters for the plugin designer.
/// In particular, we provide
/// - ip: ip address of the client
/// - sender: channel to send messages to client threads,  Sender<(String, MyMessage)>: String for IP address + port
/// - channels: all channel information
/// - my_map: all user information, and their coonnection_writes
/// It's expected that the plugin function will be FnOnce, and should not block.
/// If the plugin blocks, the sever thread will also be blocked !!! This is because we have only 1 sever thread.
/// If blocking operation is required, for example in reminder, it's expected to perform the blocking operation in a new thread.
pub fn create_plugin<'a>(
    plugin: Plugin,
    ip: String,
    sender: Sender<(String, MyMessage)>,
    channels: &'a mut HashMap<Channel, HashSet<String>>,
    my_map: &'a mut HashMap<String, ThreadInfo>,
) -> Box<dyn FnOnce() + 'a> {
    match plugin {
        Plugin::Remainder(duration, nickname, message) => {
            Box::new(move || reminder(duration, nickname, message, sender, ip))
        }
        Plugin::ListingChannels => Box::new(move || listing(ip, channels, my_map)),
    }
}
/// parse_plugin parses a raw string into a plugin enum.
pub fn parse_plugin(command: String) -> Option<Plugin> {
    if command == "LISTING" {
        return Some(Plugin::ListingChannels);
    } else if command.starts_with("REMINDER") {
        let split: Vec<&str> = command.split(" $").skip(1).collect();
        if split.len() != 3 {
            return None;
        }
        if let Ok(duration) = split[0].parse::<u64>() {
            let duration = Duration::from_secs(duration);
            let nickname = split[1].to_string();
            let message = split[2].to_string();
            return Some(Plugin::Remainder(duration, nickname, message));
        } else {
            return None;
        }
    }
    None
}

/// Doesn't need to be pub, added pub here for documentation
/// This is the function to be ran in the sever thread for the Listing Plugin
pub fn listing(
    ip: String,
    channels: &mut HashMap<Channel, HashSet<String>>,
    my_map: &mut HashMap<String, ThreadInfo>,
) {
    let channels: Vec<&Channel> = channels.keys().collect();
    let output = channels
        .iter()
        .map(|x| x.to_string())
        .collect::<Vec<String>>()
        .join(", ");
    let conn_write = &mut my_map.get_mut(&ip).unwrap().conn_write;
    conn_write
        .write_message(&format!("Available channels are [{}]\n", output))
        .unwrap();
}

/// Doesn't need to be pub
/// This is the function to be ran in the sever thread for the reminder Plugin
pub fn reminder(
    duration: Duration,
    nickname: String,
    message: String,
    sender: Sender<(String, MyMessage)>,
    ip: String,
) {
    thread::spawn(move || {
        let command = format!("PRIVMSG {} :{}", nickname, message);
        sleep(duration);
        sender.send((ip, MyMessage::Request(command))).unwrap();
    });
}
