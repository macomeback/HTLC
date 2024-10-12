mod nodes;

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::thread;
use std::thread::JoinHandle;
use std::time::Duration;
use nodes::Node;
use rand::Rng;


fn main() {
    let start_port_index = rand::thread_rng().gen_range(1024..49144);
    let mut handles: Vec<JoinHandle<_>> = vec![];
    for i in 0..4 {
        let port = start_port_index+i;
        let task = thread::spawn(move || {
            let mut new_node = Node::new(i, port);
            nodes::start(new_node);
        });
        thread::sleep(Duration::from_millis(10));
        handles.push(task);
    }
    for handle in handles {
        handle.join().unwrap();
    }
    println!("Hello, world!");
}
