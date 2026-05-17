//! Unix Domain Socket transport for tarpc IPC.
//!
//! Verbatim-ish port of `sol_ipc/src/transport.rs` from sol-os. The
//! `connect_service!` macro and daemon-specific helpers (connect_netd /
//! connect_audiod / …) are dropped — consumers build their own typed
//! clients with `<Client>::new(Config::default(), transport).spawn()`.

use tokio::net::UnixStream;

// ---------------------------------------------------------------------------
// SO_PEERCRED peer credential extraction
// ---------------------------------------------------------------------------

#[derive(Clone, Debug)]
pub struct PeerUid(pub u32);

/// Extract the peer UID from a connected Unix domain socket via `SO_PEERCRED`.
pub fn get_peer_uid(fd: std::os::unix::io::RawFd) -> Option<u32> {
    let mut ucred = libc::ucred { pid: 0, uid: 0, gid: 0 };
    let mut len = std::mem::size_of::<libc::ucred>() as libc::socklen_t;
    // SAFETY: fd is a valid connected UDS socket; ucred is properly sized and aligned.
    let ret = unsafe {
        libc::getsockopt(
            fd,
            libc::SOL_SOCKET,
            libc::SO_PEERCRED,
            &mut ucred as *mut _ as *mut libc::c_void,
            &mut len,
        )
    };
    if ret == 0 { Some(ucred.uid) } else { None }
}

// ---------------------------------------------------------------------------
// UDS binder + tarpc transport factory
// ---------------------------------------------------------------------------

/// Bind a UDS listener for a tarpc service.
/// - creates the socket directory if missing
/// - removes any stale socket file
/// - sets permissions to 0o666 (peer UID gating is done at the application layer)
pub fn bind_tarpc_socket(sock_name: &str) -> std::io::Result<tokio::net::UnixListener> {
    let dir = crate::socket_dir();
    std::fs::create_dir_all(dir)?;
    let path = crate::socket_path(sock_name);
    let _ = std::fs::remove_file(&path);
    let std_listener = std::os::unix::net::UnixListener::bind(&path)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o666));
    }
    std_listener.set_nonblocking(true)?;
    tokio::net::UnixListener::from_std(std_listener)
}

/// Create a tarpc serde transport from a tokio UnixStream.
/// Length-delimited framing + bincode codec.
pub fn make_tarpc_transport<Item, SinkItem>(
    stream: UnixStream,
) -> tarpc::serde_transport::Transport<
    UnixStream,
    Item,
    SinkItem,
    tokio_serde::formats::Bincode<Item, SinkItem>,
>
where
    Item: for<'de> serde::Deserialize<'de>,
    SinkItem: serde::Serialize,
{
    tarpc::serde_transport::Transport::from((stream, tokio_serde::formats::Bincode::default()))
}

/// Connect to a tarpc service over UDS, returning the raw tokio UnixStream.
pub async fn connect_tarpc_raw(sock_name: &str) -> std::io::Result<UnixStream> {
    let path = crate::socket_path(sock_name);
    UnixStream::connect(&path).await
}
