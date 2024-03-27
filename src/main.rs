use std::io::{self, Write};
use std::task::Poll;
use std::{env, process};

use pnet::datalink::{self, Channel};
use pnet::packet::ethernet::EthernetPacket;

use pnet::packet::ipv4::Ipv4Packet;
use pnet::packet::tcp::TcpPacket;
use pnet::packet::Packet;
use tokio::io::{AsyncRead, AsyncReadExt};
use tokio::spawn;
use tokio::sync::mpsc::{unbounded_channel, UnboundedReceiver, UnboundedSender};

#[tokio::main(flavor = "multi_thread", worker_threads = 3)]
async fn main() {
    {
        let mut l = io::stdout().lock();
        writeln!(l, "rcsniff2 [optional network interface name]").unwrap();
        writeln!(
            l,
            "valid interfaces: {:#?}",
            datalink::interfaces()
                .into_iter()
                .map(|i| i.name)
                .collect::<Vec<_>>()
        )
        .unwrap();
    }

    let Some(iface_name) = env::args()
        .nth(1)
        .or(netdev::get_default_interface().ok().map(|iface| iface.name))
    else {
        let mut l = io::stderr().lock();
        writeln!(l, "failed to get default interface name and you haven't specified one. Please specify the network interface to use").unwrap();
        process::exit(1);
    };

    let incoming_packet_tx = make_incoming_handler();
    let outgoing_packet_tx = make_outgoing_handler();
    let int = datalink::interfaces()
        .into_iter()
        .filter(|s| s.name == iface_name)
        .next()
        .unwrap();
    let Ok(Channel::Ethernet(_, mut rx)) = datalink::channel(&int, Default::default()) else {
        panic!("could not create channel listener")
    };
    loop {
        if let Ok(packet) = rx.next() {
            if let Some(ipv4) = Ipv4Packet::new(EthernetPacket::new(packet).unwrap().payload()) {
                if let Some(tcp) = TcpPacket::new(ipv4.payload()) {
                    if tcp.get_source() == 4533 {
                        let v = tcp.payload().to_vec();
                        incoming_packet_tx.send(v).unwrap();
                    }
                    if tcp.get_destination() == 4533 {
                        let v = tcp.payload().to_vec();
                        outgoing_packet_tx.send(v).unwrap();
                    }
                }
            }
        }
    }
}

fn make_incoming_handler() -> UnboundedSender<Vec<u8>> {
    let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
    let (packet_tx, packet_rx) = unbounded_channel();
    spawn(incoming_reciever_thread(tx, ByteReciever::new(packet_rx)));
    spawn(handler::handler_thread(rx));
    packet_tx
}

fn make_outgoing_handler() -> UnboundedSender<Vec<u8>> {
    let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
    let (packet_tx, packet_rx) = unbounded_channel();
    spawn(outgoing_reciever_thread(tx, ByteReciever::new(packet_rx)));
    spawn(handler::handler_thread(rx));
    packet_tx
}
pub struct ByteReciever {
    buf: Vec<u8>,
    rx: UnboundedReceiver<Vec<u8>>,
}
impl ByteReciever {
    pub fn new(rx: UnboundedReceiver<Vec<u8>>) -> Self {
        Self {
            buf: Vec::with_capacity(100),
            rx,
        }
    }
}
impl AsyncRead for ByteReciever {
    fn poll_read(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
        buf: &mut tokio::io::ReadBuf<'_>,
    ) -> std::task::Poll<io::Result<()>> {
        let mut p = self.rx.poll_recv(cx);
        loop {
            match p {
                Poll::Ready(v) => {
                    if v.is_some() && v.as_ref().map(|v| v.len()).unwrap() != 0 {
                        self.buf.extend(v.unwrap().into_iter());
                        break;
                    } else {
                        p = self.rx.poll_recv(cx);
                    }
                }
                Poll::Pending => return Poll::Pending,
            }
        }
        if self.buf.len() != 0 {
            let to_copy = buf.remaining().min(self.buf.len());
            let buf2 = self.buf.split_off(to_copy);
            buf.put_slice(&self.buf);
            self.buf = buf2;
            cx.waker().wake_by_ref();
            Poll::Ready(Ok(()))
        } else {
            Poll::Pending
        }
    }
}
async fn incoming_reciever_thread(
    sender: UnboundedSender<Vec<u8>>,
    mut recv: impl AsyncReadExt + Unpin,
) {
    let mut buf = Vec::with_capacity(1200);

    loop {
        buf.clear();
        let mut arr1 = [0u8; 9];
        let Ok(_) = recv.read_exact(&mut arr1).await else {
            break;
        };
        let arr2 = [arr1[1], arr1[2], arr1[3], arr1[4]];
        if arr1[0] == 0xFB {
            let mut total_packet_len = i32::from_be_bytes(arr2);
            buf.extend_from_slice(&arr1[7..]);
            total_packet_len -= 9;
            let mut buf2 = vec![0u8; total_packet_len as usize];
            recv.read_exact(&mut buf2).await.unwrap();
            buf.extend_from_slice(&buf2);
            sender.send(buf.clone()).unwrap();
        }
    }
}
async fn outgoing_reciever_thread(
    sender: UnboundedSender<Vec<u8>>,
    mut recv: impl AsyncReadExt + Unpin,
) {
    let mut buf = Vec::with_capacity(1200);

    loop {
        buf.clear();
        let mut arr1 = [0u8; 5];
        let Ok(_) = recv.read_exact(&mut arr1).await else {
            break;
        };
        let arr2 = [arr1[1], arr1[2], arr1[3], arr1[4]];

        if arr1[0] == 0xFB {
            let mut total_packet_len = i32::from_be_bytes(arr2);
            total_packet_len -= 5;
            let mut buf2 = vec![0u8; total_packet_len as usize];
            recv.read_exact(&mut buf2).await.unwrap();
            buf.extend_from_slice(&buf2[2..]);
            sender.send(buf.clone()).unwrap();
        }
    }
}
pub mod handler {
    use std::io::{Cursor, Seek};

    use tokio::sync::mpsc::UnboundedReceiver;

    use rcfakeclient::serialization::{op_code::MessageType, StreamDeserializer};

    pub async fn handler_thread(mut rec: UnboundedReceiver<Vec<u8>>) {
        while let Some(buf) = rec.recv().await {
            if buf[0] != 0xF3 {
                continue;
            }
            let code = buf[1];
            let encflag = (code & 128) != 0;
            if encflag {
                println!("ERROR: encountered encrypted packet. This program cannot decrypt encrypted packets");
                continue;
            }
            let b2 = buf.clone();
            let mut buf = Cursor::new(&buf[..]);
            buf.seek(std::io::SeekFrom::Start(2)).unwrap();

            let msg_type = code & 127;
            let mut des = StreamDeserializer::new(buf);
            match MessageType::from_repr(msg_type).unwrap() {
                MessageType::Init => {
                    // init request
                }
                MessageType::InitResponse => { //Init response
                }
                MessageType::Operation => {
                    //Operation Request
                    let res = des.deserialize_operation_request();
                    println!("Operation Request: {:#?}", res);
                    if res.is_err() {
                        println!("Erroring Request: {:x?}", b2);
                    }
                }
                MessageType::OperationResponse => {
                    //Operation response
                    let res = des.deserialize_operation_response();
                    println!("Operation Response: {:#?}", res);
                    if res.is_err() {
                        println!("Erroring Request: {:x?}", b2);
                    }
                }
                MessageType::Event => {
                    //EventData
                    println!("Event Data: {:#?}", des.deserialize_event_data());
                }
                MessageType::InternalOperationRequest => {
                    let res = des.deserialize_operation_request();
                    println!("Internal Operation Request: {:#?}", res);
                    //Internal operation request
                }
                MessageType::InternalOperationResponse => {
                    //Internal operation response
                    let res = des.deserialize_operation_response();
                    println!("Internal Operation Response: {:#?}", res);
                }
                MessageType::Message => {
                    //Message
                }
                MessageType::RawMessage => {
                    //Raw message
                }
            }
        }
    }
}
