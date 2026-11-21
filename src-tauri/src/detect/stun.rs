//! UDP 出口检测（等价于浏览器 WebRTC 泄露检测）：向 STUN 服务器发 Binding Request，
//! 读取 XOR-MAPPED-ADDRESS 得到 UDP 流量的公网出口 IP。
use serde::Serialize;
use std::net::{Ipv4Addr, SocketAddr};
use std::time::Duration;
use tokio::net::UdpSocket;

const MAGIC: u32 = 0x2112_A442;

#[derive(Debug, Clone, Serialize, Default)]
pub struct Udp {
    pub ip: String,
    pub server: String,
    pub country_code: String,
}

pub async fn detect(servers: &[String], timeout_ms: u64) -> anyhow::Result<Udp> {
    let mut last_err = anyhow::anyhow!("未配置 STUN 服务器");
    for s in servers {
        match query(s, timeout_ms).await {
            Ok(ip) => return Ok(Udp { ip: ip.to_string(), server: s.clone(), country_code: String::new() }),
            Err(e) => last_err = e,
        }
    }
    Err(last_err)
}

async fn query(server: &str, timeout_ms: u64) -> anyhow::Result<Ipv4Addr> {
    let addr: SocketAddr = tokio::net::lookup_host(server)
        .await?
        .find(|a| a.is_ipv4())
        .ok_or_else(|| anyhow::anyhow!("{server} 无 IPv4 地址"))?;
    let sock = UdpSocket::bind("0.0.0.0:0").await?;
    sock.connect(addr).await?;

    let txid: [u8; 12] = rand::random();
    let mut req = Vec::with_capacity(20);
    req.extend_from_slice(&0x0001u16.to_be_bytes()); // Binding Request
    req.extend_from_slice(&0u16.to_be_bytes()); // length
    req.extend_from_slice(&MAGIC.to_be_bytes());
    req.extend_from_slice(&txid);

    let mut buf = [0u8; 1024];
    for _ in 0..3 {
        sock.send(&req).await?;
        if let Ok(Ok(n)) = tokio::time::timeout(Duration::from_millis(timeout_ms / 3), sock.recv(&mut buf)).await {
            if let Some(ip) = parse(&buf[..n], &txid) {
                return Ok(ip);
            }
        }
    }
    anyhow::bail!("{server} 无响应（UDP 可能被阻断）")
}

fn parse(p: &[u8], txid: &[u8; 12]) -> Option<Ipv4Addr> {
    if p.len() < 20 || p[0..2] != [0x01, 0x01] || &p[8..20] != txid {
        return None;
    }
    let len = u16::from_be_bytes([p[2], p[3]]) as usize;
    let mut i = 20;
    let end = (20 + len).min(p.len());
    let mut mapped = None;
    while i + 4 <= end {
        let t = u16::from_be_bytes([p[i], p[i + 1]]);
        let l = u16::from_be_bytes([p[i + 2], p[i + 3]]) as usize;
        let v = p.get(i + 4..i + 4 + l)?;
        if l >= 8 && v[1] == 0x01 {
            let raw = u32::from_be_bytes([v[4], v[5], v[6], v[7]]);
            match t {
                0x0020 => return Some(Ipv4Addr::from(raw ^ MAGIC)), // XOR-MAPPED-ADDRESS
                0x0001 => mapped = Some(Ipv4Addr::from(raw)),      // MAPPED-ADDRESS
                _ => {}
            }
        }
        i += 4 + ((l + 3) & !3);
    }
    mapped
}
