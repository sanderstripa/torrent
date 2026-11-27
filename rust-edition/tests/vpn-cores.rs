#![cfg(all(windows, feature = "vpn"))]

use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine as _};
use serde_json::json;
use std::error::Error;
use std::io::{Read, Write};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::path::Path;
use std::process::{Child, Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};

type Result<T> = std::result::Result<T, Box<dyn Error>>;

struct Process(Child);
impl Drop for Process {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

fn unused_port() -> Result<u16> {
    Ok(TcpListener::bind("127.0.0.1:0")?.local_addr()?.port())
}

fn wait_for(mut condition: impl FnMut() -> bool) -> Result<()> {
    let deadline = Instant::now() + Duration::from_secs(15);
    while Instant::now() < deadline {
        if condition() {
            return Ok(());
        }
        thread::sleep(Duration::from_millis(100));
    }
    Err("condition timed out".into())
}

#[test]
#[ignore = "requires the pinned Xray and Sudoku Windows binaries"]
fn pinned_cores_forward_and_stop_with_the_helper() -> Result<()> {
    let package = std::env::var_os("TORRENT_VPN_FIXTURE_BIN").ok_or("missing VPN package path")?;
    for core in ["xray", "sudoku"] {
        run_core(Path::new(&package), core)?;
    }
    Ok(())
}

fn run_core(package: &Path, core: &str) -> Result<()> {
    let workspace = tempfile::tempdir()?;
    let server_port = unused_port()?;
    let id = "12345678-1234-1234-1234-123456789abc";
    let secret = "ab".repeat(32);

    let (server, link, arguments) = if core == "xray" {
        (
            json!({
                "inbounds": [{
                    "listen": "127.0.0.1", "port": server_port, "protocol": "vless",
                    "settings": {"clients": [{"id": id}], "decryption": "none"}
                }],
                "outbounds": [{
                    "protocol": "freedom",
                    "settings": {"finalRules": [{"action": "allow", "ip": ["127.0.0.1/32"]}]}
                }]
            }),
            format!("vless://{id}@127.0.0.1:{server_port}?security=none&type=tcp"),
            &["run", "-config"][..],
        )
    } else {
        let key = json!({"h": "127.0.0.1", "p": server_port, "k": secret, "e": "none", "hd": true});
        (
            json!({
                "mode": "server", "transport": "tcp", "local_port": server_port,
                "key": secret, "aead": "none", "padding_min": 5, "padding_max": 15,
                "ascii": "prefer_entropy", "enable_pure_downlink": true,
                "suspicious_action": "silent", "httpmask": {"disable": true}, "rule_urls": ["global"]
            }),
            format!("sudoku://{}", URL_SAFE_NO_PAD.encode(key.to_string())),
            &["-c"][..],
        )
    };
    let configuration = workspace.path().join("server.json");
    std::fs::write(&configuration, serde_json::to_vec(&server)?)?;
    let core_directory = package.join(core);
    let _server = Process(
        Command::new(core_directory.join(format!("{core}.exe")))
            .current_dir(&core_directory)
            .args(arguments)
            .arg(&configuration)
            .stdin(Stdio::null())
            .spawn()?,
    );
    wait_for(|| TcpStream::connect(("127.0.0.1", server_port)).is_ok())?;

    let port = unused_port()?;
    let key_path = workspace.path().join("key.txt");
    let status_path = workspace.path().join("status.txt");
    std::fs::write(&key_path, link)?;
    let mut helper = Process(
        Command::new(package.join("TorrentVpnHost.exe"))
            .arg("--run")
            .arg(&key_path)
            .arg(&status_path)
            .arg(port.to_string())
            .stdin(Stdio::piped())
            .spawn()?,
    );
    wait_for(|| std::fs::read_to_string(&status_path).is_ok_and(|s| s == "ready")).map_err(
        |error| {
            format!(
                "{core}: {error} ({:?})",
                std::fs::read_to_string(&status_path)
            )
        },
    )?;
    assert!(!key_path.exists());

    let target = TcpListener::bind("127.0.0.1:0")?;
    let destination_port = target.local_addr()?.port();
    let proxy = SocketAddr::from(([127, 0, 0, 1], port));
    let duration = Duration::from_secs(5);
    let mut stream = TcpStream::connect_timeout(&proxy, duration)?;
    stream.set_read_timeout(Some(duration))?;
    stream.set_write_timeout(Some(duration))?;
    stream.write_all(&[5, 1, 0])?;
    let mut authentication = [0; 2];
    stream.read_exact(&mut authentication)?;
    assert_eq!(authentication, [5, 0]);
    let [high, low] = destination_port.to_be_bytes();
    stream.write_all(&[5, 1, 0, 1, 127, 0, 0, 1, high, low])?;
    let mut reply = [0; 4];
    stream.read_exact(&mut reply)?;
    assert_eq!(&reply[..2], &[5, 0], "{core}");
    let address_length = match reply[3] {
        1 => 4,
        4 => 16,
        3 => {
            let mut length = [0];
            stream.read_exact(&mut length)?;
            usize::from(length[0])
        }
        _ => return Err("unexpected SOCKS binding address".into()),
    };
    stream.read_exact(&mut vec![0; address_length + 2])?;
    target.set_nonblocking(true)?;
    let mut receiver = None;
    wait_for(|| match target.accept() {
        Ok((connection, _)) => {
            receiver = Some(connection);
            true
        }
        Err(_) => false,
    })?;
    let mut receiver = receiver.unwrap();
    receiver.set_nonblocking(false)?;
    receiver.set_write_timeout(Some(duration))?;
    receiver.write_all(b"torrent-vpn-fixture")?;
    let mut payload = [0; 19];
    stream.read_exact(&mut payload)?;
    assert_eq!(&payload, b"torrent-vpn-fixture");

    // Terminating the helper also terminates the core and closes live tunnel sockets.
    #[cfg(feature = "rqbit")]
    check_active_transfer(port, workspace.path(), &mut helper.0)?;
    #[cfg(not(feature = "rqbit"))]
    {
        helper.0.kill()?;
        helper.0.wait()?;
    }
    wait_for(|| TcpStream::connect_timeout(&proxy, Duration::from_millis(200)).is_err())?;
    assert!(!matches!(stream.read(&mut [0]), Ok(n) if n > 0));
    Ok(())
}

#[cfg(feature = "rqbit")]
fn check_active_transfer(port: u16, directory: &Path, helper: &mut Child) -> Result<()> {
    use torrent_re_sort::{
        engine::{Network, Session},
        queue_policy::PREVIEW_POLICY,
        transfers::Transfers,
    };

    let tracker = TcpListener::bind("127.0.0.1:0")?;
    let peer = TcpListener::bind("127.0.0.1:0")?;
    tracker.set_nonblocking(true)?;
    peer.set_nonblocking(true)?;
    let tracker_url = format!("http://{}/announce", tracker.local_addr()?);
    let mut torrent = format!("d8:announce{}:{tracker_url}4:infod6:lengthi32768e4:name7:fixture12:piece lengthi16384e6:pieces40:", tracker_url.len()).into_bytes();
    torrent.extend(hex::decode(
        "12ff347b4f27d69e1f328e6f4b5573e3666e122f76024243187a14d9cfa7ed6f212c8151b72f7b92",
    )?);
    torrent.extend_from_slice(b"ee");
    let fixture = directory.join("vpn-transfer.torrent");
    let output = directory.join("downloads");
    std::fs::write(&fixture, &torrent)?;
    let session = Session::open(PREVIEW_POLICY, true)?;
    let (mut transfers, warnings) = Transfers::open(&session, &directory.join("queue"))?;
    assert!(warnings.is_empty());
    transfers.change_network(Network::Proxy(port))?;
    transfers.add(fixture.to_str().unwrap(), output.to_str().unwrap(), false)?;
    let entry = &transfers.ordered()[0].handle;
    // Poll the app's normal status path until it has started the torrent.
    wait_for(|| entry.status().is_ok_and(|(_, status)| status.has_metadata))?;

    let mut request = accept(&tracker)?;
    let mut request_bytes = Vec::new();
    while request_bytes.len() < 8192 && !request_bytes.ends_with(b"\r\n\r\n") {
        let mut byte = [0];
        request.read_exact(&mut byte)?;
        request_bytes.push(byte[0]);
    }
    assert!(request_bytes.starts_with(b"GET /announce?"));
    let mut body = b"d8:intervali1e5:peers6:".to_vec();
    body.extend_from_slice(&[127, 0, 0, 1]);
    body.extend_from_slice(&peer.local_addr()?.port().to_be_bytes());
    body.push(b'e');
    write!(
        request,
        "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        body.len()
    )?;
    request.write_all(&body)?;
    drop(request);

    let mut connection = accept(&peer)?;
    let mut handshake = [0; 68];
    connection.read_exact(&mut handshake)?;
    assert_eq!(&handshake[1..20], b"BitTorrent protocol");
    handshake[20..28].fill(0);
    handshake[48..68].copy_from_slice(b"-XX0001-VPNTEST00001");
    connection.write_all(&handshake)?;
    connection.write_all(&[0, 0, 0, 2, 5, 0xc0, 0, 0, 0, 1, 1])?;
    loop {
        let mut length = [0; 4];
        connection.read_exact(&mut length)?;
        let length = u32::from_be_bytes(length) as usize;
        if length == 0 {
            continue;
        }
        assert!(length < 32 * 1024);
        let mut message = vec![0; length];
        connection.read_exact(&mut message)?;
        if message[0] == 6 {
            let piece = u32::from_be_bytes(message[1..5].try_into()?);
            let offset = u32::from_be_bytes(message[5..9].try_into()?);
            let requested = u32::from_be_bytes(message[9..13].try_into()?);
            assert_eq!((offset, requested), (0, 16384));
            assert!(piece < 2);
            connection.write_all(&(9 + requested).to_be_bytes())?;
            connection.write_all(&[7])?;
            connection.write_all(&piece.to_be_bytes())?;
            connection.write_all(&offset.to_be_bytes())?;
            connection.write_all(&vec![b'a' + piece as u8; requested as usize])?;
            break;
        }
    }
    wait_for(|| {
        entry
            .status()
            .is_ok_and(|(_, status)| status.wanted_done == 16384)
    })?;

    helper.kill()?;
    helper.wait()?;
    let mut trailing = Vec::new();
    match connection.read_to_end(&mut trailing) {
        Ok(_) => {}
        Err(error)
            if matches!(
                error.kind(),
                std::io::ErrorKind::ConnectionReset | std::io::ErrorKind::ConnectionAborted
            ) => {}
        Err(error) => {
            return Err(format!("The BitTorrent connection survived VPN loss: {error}").into())
        }
    }
    for _ in 0..20 {
        assert!(
            matches!(peer.accept(), Err(error) if error.kind() == std::io::ErrorKind::WouldBlock)
        );
        assert!(
            matches!(tracker.accept(), Err(error) if error.kind() == std::io::ErrorKind::WouldBlock)
        );
        assert_eq!(entry.status()?.1.wanted_done, 16384);
        thread::sleep(Duration::from_millis(100));
    }
    transfers.change_network(Network::Blocked)?;
    Ok(())
}

#[cfg(feature = "rqbit")]
fn accept(listener: &TcpListener) -> Result<TcpStream> {
    let mut result = None;
    wait_for(|| match listener.accept() {
        Ok((stream, _)) => {
            result = Some(stream);
            true
        }
        Err(_) => false,
    })?;
    let stream = result.ok_or("Loopback connection did not arrive")?;
    stream.set_nonblocking(false)?;
    stream.set_read_timeout(Some(Duration::from_secs(5)))?;
    stream.set_write_timeout(Some(Duration::from_secs(5)))?;
    Ok(stream)
}
