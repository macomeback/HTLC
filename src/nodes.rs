use std::io::{BufRead, BufReader, Read, Write};
use std::net::{Ipv4Addr, SocketAddrV4};
use std::net::Shutdown;
use std::thread;
use std::net::{TcpListener, TcpStream};
use rand::Rng;
use std::collections::HashMap;
use std::hash::{DefaultHasher, Hash, Hasher};
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime};
use serde_json::{json, Value};

// const ADDR: Ipv4Addr = Ipv4Addr::LOCALHOST;
const ADDR: Ipv4Addr = Ipv4Addr::new(127, 0, 0, 1);

fn decode(encoded_str: &str) -> Value {
    let msg = serde_json::from_str(encoded_str);
    match msg {
        Ok(msg_value) => msg_value,
        Err(e) => {println!("failed to decode {}", encoded_str); json!({})},
    }
}

fn hash(pre_image: u64) -> u64 {
    let mut hasher = DefaultHasher::new();
    pre_image.hash(&mut hasher);
    hasher.finish()
}

fn next_index(index: u64) -> u64 {
    match index {
        3 => 0,
        _ => index + 1,
    }
}

fn prev_index(index: u64) -> u64 {
    match index {
        0 => 3,
        _ => index - 1,
    }
}


pub struct Node {
    pub index: u64,
    pub port: u64,
    pub balance: u64,
    pub deposit: u64,
    pub pre_image: u64,
    pub hash_value: u64,
    pub prev_connection: Option<TcpStream>,
    pub next_connection: Option<TcpStream>,
    pub deadline: SystemTime,
}

impl Node {
    pub fn new(index: u64, port: u64) -> Self {
        Self { index, port, balance: 10, deposit: 0, pre_image: 0, hash_value: 0, prev_connection: None, next_connection: None, deadline: SystemTime::now() }
    }
}

pub fn start(mut node: Node) {
    let port = node.port;
    let index = node.index;
    if prev_index(index) < index {
        connect(&mut node, port + prev_index(index) - index, prev_index(index));
    }
    if next_index(index) < index {
        connect(&mut node, port + next_index(index) - index, next_index(index));
    }
    listen(Arc::new(Mutex::new(node)), port);
}

fn introduce(node: &mut Node, next: bool) {
    let json_msg = json!({"type":"introduction", "port": node.port, "index": node.index});
    if next {
        send_message_to_next(node, json_msg);
    } else {
        send_message_to_prev(node, json_msg);
    }
    println!("Introduce {} to next: {} or prev: {}", node.index, next, !next);
}

fn connect(node: &mut Node, port: u64, index: u64) {
    println!("Hello Client!");
    if let Ok(mut stream) = TcpStream::connect(SocketAddrV4::new(ADDR, port as u16)) {
        println!("Connected to the server on {:?}", stream.peer_addr().unwrap());
        if next_index(index) == node.index {
            node.prev_connection = Some(stream);
            introduce(node, false);
        } else if next_index(node.index) == index {
            node.next_connection = Some(stream);
            introduce(node, true);
            if node.index == 3 {
                send_hash_value(node);
            }
        }
    } else {
        println!("Couldn't connect to server...");
    }
}

fn send_hash_value(node: &mut Node) {
    node.pre_image = rand::thread_rng().gen_range(0..u64::MAX);
    let hash_value = hash(node.pre_image);
    let json_msg = json!({"type":"hash_value", "hash_value":hash_value});
    println!("hash message sent");
    send_message_to_next(node, json_msg);
}

fn listen(this: Arc<Mutex<Node>>, port: u64) {
    let listener = TcpListener::bind(SocketAddrV4::new(ADDR, port as u16)).unwrap();
    println!("{:?}", listener);
    for stream in listener.incoming() {
        match stream {
            Ok(mut stream) => {
                println!("New connection: {}", stream.peer_addr().unwrap());
                let clone = Arc::clone(&this);
                thread::spawn(move || {
                    handle_stream(clone, stream);
                });
            }
            Err(err) => println!("Connection failed due to {:?}", err)
        }
    }
}

fn send_message_to_next(node: &mut Node, json_msg: Value) {
    let stream = node.next_connection.as_mut().unwrap();
    send_message_to_stream(json_msg, stream);
}

fn send_message_to_prev(node: &mut Node, json_msg: Value) {
    let stream = node.prev_connection.as_mut().unwrap();
    send_message_to_stream(json_msg, stream);
}

fn send_message_to_stream(json_msg: Value, stream: &mut TcpStream) {
    let mut encoded_msg = serde_json::to_string(&json_msg).unwrap();
    encoded_msg = encoded_msg + "\n";
    stream.write(encoded_msg.as_bytes()).unwrap();
    stream.flush();
}

fn handle_stream(this: Arc<Mutex<Node>>, mut stream: TcpStream) {
    let mut first_received = false;
    let mut index = 0;
    let mut reader = BufReader::new(&stream);
    let mut data = String::new();
    while match reader.read_line(&mut data) {
        Ok(size) => {
            if size > 0 {
                let mut node = this.lock().unwrap();
                let json_msg = decode(&data);
                if !first_received {
                    index = handle_introduction(&mut node, json_msg);
                    first_received = true;
                } else {
                    handle_msg(&mut node, json_msg, index);
                }
                drop(node);
                data = String::new();
            }
            true
        }
        Err(_) => {
            println!("An error occurred, terminating connection with {}", stream.peer_addr().unwrap());
            stream.shutdown(Shutdown::Both).unwrap();
            false
        }
    } {}
}

fn htlc(node: &mut Node, hash_value: u64, amount: u64, time_due: u64) {
    if node.index != 3 {
        let htlc_msg = json!({
                "type": "htlc",
                "hash_value": hash_value,
                "amount": amount,
                "time_due": time_due
            });
        node.balance -= amount;
        node.deposit += amount;
        node.hash_value = hash_value;
        node.deadline = SystemTime::now()+Duration::new(time_due, 0);
        send_message_to_next(node, htlc_msg);
    }
}

fn handle_htlc(node: &mut Node, json_msg: Value, index: u64) {
    if node.index<3 && node.index == index+1 {
        let amount = json_msg["amount"].as_u64().unwrap();
        let time_due = json_msg["time_due"].as_u64().unwrap();
        htlc(node, json_msg["hash_value"].as_u64().unwrap(), amount, time_due - 1);
    } else if node.index == 3 && index == 2 {
        claim(node, node.pre_image);
    }
}

fn claim(node: &mut Node, pre_image: u64) {
    let json_claim = json!({
                "type": "claim",
                "pre_image": pre_image,
            });
    send_message_to_prev(node, json_claim);
}

fn handle_claim(node: &mut Node, json_msg: Value, index: u64) {
    if node.index < 3 && node.index == index - 1 {
        let pre_image = json_msg["pre_image"].as_u64().unwrap();
        if hash(pre_image) == node.hash_value {
            if index != 0 {
                claim(node, pre_image);
            }
            if node.deadline > SystemTime::now(){
                pay_deposit(node);
            }
        }
    }
}

fn pay_deposit(node: &mut Node) {
    let json_pay = json!({
           "type": "payment",
            "amount": node.deposit,
        });
    node.deposit = 0;
    send_message_to_next(node, json_pay);
}

fn handle_payment(node: &mut Node, json_msg: Value) {
    let amount = json_msg["amount"].as_u64().unwrap();
    node.balance += amount;
}
fn handle_introduction(node: &mut Node, json_msg: Value) -> u64 {
    let index = json_msg["index"].as_u64().unwrap();
    if index > node.index {
        connect(node, json_msg["port"].as_u64().unwrap(), index);
    }
    println!("Introduction received on {} from {}", node.index, index);
    index
}
fn handle_msg(node: &mut Node, json_msg: Value, index: u64) {
    let msg_type = json_msg["type"].as_str().unwrap();
    println!("Received message of type {} on node {} from node {}", msg_type, index, node.index);
    match msg_type {
        "hash_value" => if node.index == 0 && index == 3 {
            htlc(node, json_msg["hash_value"].as_u64().unwrap(), 1, 5);
        },
        "htlc" => handle_htlc(node, json_msg, index),
        "claim" => handle_claim(node, json_msg, index),
        "payment" => handle_payment(node, json_msg),
        _ => (),
    }
}

