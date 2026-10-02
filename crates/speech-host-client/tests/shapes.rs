use std::path::PathBuf;

use speech_host_client::{HostSocket, SpeechHostClient};

#[test]
fn a_client_is_built_from_a_socket() {
    let socket = HostSocket(PathBuf::from("/run/user/1000/inferd/speech-host.sock"));
    let client = SpeechHostClient::new(socket.clone());
    assert_eq!(client.socket(), &socket);
}

#[test]
fn a_socket_is_its_path_in_json() {
    let socket = HostSocket(PathBuf::from("/run/inferd/a.sock"));
    assert_eq!(
        serde_json::to_string(&socket).unwrap(),
        r#""/run/inferd/a.sock""#
    );
}
