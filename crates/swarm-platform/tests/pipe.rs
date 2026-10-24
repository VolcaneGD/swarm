use std::io::{Read, Write};
use swarm_platform::{PlatformError, pipe};

#[test]
fn endpoint_validation_rejects_outside_names_and_oversize() {
    for name in ["", "other-pipe", "Swarm-../outside", "Swarm-nul\0x"] {
        assert!(matches!(
            pipe::Listener::bind(name),
            Err(PlatformError::InvalidEndpoint)
        ));
    }
    assert!(matches!(
        pipe::connect(&format!("Swarm-{}", "a".repeat(123))),
        Err(PlatformError::InvalidEndpoint)
    ));
}

#[cfg(windows)]
#[test]
fn real_pipe_round_trip_and_singleton_binding() {
    let name = format!("Swarm-{}", uuid::Uuid::new_v4().simple());
    let mut listener = pipe::Listener::bind(&name).unwrap();
    assert!(pipe::Listener::bind(&name).is_err());

    let client_name = name.clone();
    let client = std::thread::spawn(move || {
        let mut connection = pipe::connect(&client_name).unwrap();
        connection.write_all(b"ping").unwrap();
        let mut response = [0; 4];
        connection.read_exact(&mut response).unwrap();
        response
    });

    let mut connection = listener.accept().unwrap();
    let mut request = [0; 4];
    connection.read_exact(&mut request).unwrap();
    assert_eq!(&request, b"ping");
    connection.write_all(b"pong").unwrap();
    assert_eq!(client.join().unwrap(), *b"pong");
}

#[cfg(windows)]
#[test]
fn accept_until_preserves_blocking_connection_io() {
    use std::sync::atomic::AtomicBool;

    let name = format!("Swarm-{}", uuid::Uuid::new_v4().simple());
    let mut listener = pipe::Listener::bind(&name).unwrap();
    let client_name = name.clone();
    let client = std::thread::spawn(move || {
        let mut connection = pipe::connect(&client_name).unwrap();
        connection.write_all(b"ping").unwrap();
        let mut response = [0; 4];
        connection.read_exact(&mut response).unwrap();
        response
    });

    let mut connection = listener
        .accept_until(&AtomicBool::new(false))
        .unwrap()
        .expect("the client should be accepted");
    let mut request = [0; 4];
    connection.read_exact(&mut request).unwrap();
    assert_eq!(&request, b"ping");
    connection.write_all(b"pong").unwrap();
    assert_eq!(client.join().unwrap(), *b"pong");
}

#[cfg(windows)]
#[test]
fn idle_accept_until_cancels_quickly_and_releases_the_listener_instance() {
    use std::sync::Arc;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::time::{Duration, Instant};

    let name = format!("Swarm-{}", uuid::Uuid::new_v4().simple());
    let mut listener = pipe::Listener::bind(&name).unwrap();
    let stopping = Arc::new(AtomicBool::new(false));
    let signal = Arc::clone(&stopping);
    let setter = std::thread::spawn(move || {
        std::thread::sleep(Duration::from_millis(100));
        signal.store(true, Ordering::Release);
    });

    let started = Instant::now();
    assert!(listener.accept_until(&stopping).unwrap().is_none());
    assert!(started.elapsed() < Duration::from_secs(1));
    setter.join().unwrap();

    // Cancellation must close the reserved server handle even while the
    // Listener value remains alive in this scope.
    let replacement = pipe::Listener::bind(&name).unwrap();
    drop(replacement);
}

#[cfg(windows)]
#[test]
fn client_closed_before_accept_does_not_terminate_the_listener() {
    let name = format!("Swarm-{}", uuid::Uuid::new_v4().simple());
    let mut listener = pipe::Listener::bind(&name).unwrap();
    // Deterministically close the first client before ConnectNamedPipe runs.
    drop(pipe::connect(&name).unwrap());
    let closed = listener.accept().unwrap();
    assert!(pipe::Listener::bind(&name).is_err());
    drop(closed);
    let client_name = name.clone();
    let client = std::thread::spawn(move || {
        let mut connection = pipe::connect(&client_name).unwrap();
        connection.write_all(b"live").unwrap();
        let mut reply = [0; 4];
        connection.read_exact(&mut reply).unwrap();
        reply
    });
    let mut next = listener.accept().unwrap();
    let mut request = [0; 4];
    next.read_exact(&mut request).unwrap();
    assert_eq!(&request, b"live");
    next.write_all(b"okay").unwrap();
    assert_eq!(client.join().unwrap(), *b"okay");
}

#[cfg(windows)]
#[test]
fn idle_read_returns_with_a_timeout_error() {
    use std::sync::mpsc;
    use std::time::{Duration, Instant};

    let name = format!("Swarm-{}", uuid::Uuid::new_v4().simple());
    let mut listener = pipe::Listener::bind(&name).unwrap();
    let client_name = name.clone();
    let (connected_tx, connected_rx) = mpsc::channel();
    let (release_tx, release_rx) = mpsc::channel();
    let client = std::thread::spawn(move || {
        let _connection = pipe::connect(&client_name).unwrap();
        connected_tx.send(()).unwrap();
        release_rx.recv().unwrap();
    });

    let mut connection = listener.accept().unwrap();
    connected_rx.recv().unwrap();
    connection.set_timeout(Duration::from_secs(1));
    let started = Instant::now();
    let mut byte = [0];
    let error = connection.read(&mut byte).unwrap_err();
    assert_eq!(error.kind(), std::io::ErrorKind::TimedOut);
    assert!(started.elapsed() < Duration::from_secs(3));
    release_tx.send(()).unwrap();
    client.join().unwrap();
}

#[cfg(windows)]
#[test]
fn connection_deadline_covers_time_between_reads() {
    use std::sync::mpsc;
    use std::time::{Duration, Instant};

    let name = format!("Swarm-{}", uuid::Uuid::new_v4().simple());
    let mut listener = pipe::Listener::bind(&name).unwrap();
    let client_name = name.clone();
    let (first_byte_tx, first_byte_rx) = mpsc::channel();
    let client = std::thread::spawn(move || {
        let mut connection = pipe::connect(&client_name).unwrap();
        connection.write_all(b"a").unwrap();
        first_byte_tx.send(()).unwrap();
        std::thread::sleep(Duration::from_millis(1_500));
        let _ = connection.write_all(b"b");
    });

    let mut connection = listener.accept().unwrap();
    first_byte_rx.recv().unwrap();
    connection.set_io_timeout(Duration::from_secs(5));
    connection.set_deadline(Duration::from_secs(1));
    let started = Instant::now();
    let mut bytes = [0; 2];
    let error = connection.read_exact(&mut bytes).unwrap_err();
    assert_eq!(error.kind(), std::io::ErrorKind::TimedOut);
    assert!(started.elapsed() < Duration::from_millis(2_200));
    client.join().unwrap();
}
