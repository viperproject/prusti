// © 2020, ETH Zurich
//
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

use super::PrustiServer;
use prusti_viper::verification_service::*;

use futures::Future;
use std::net::SocketAddr;
use std::sync::{mpsc, Arc};
use std::thread;
use tarpc::sync::client::ClientExt;
use tarpc::sync::{client, server};
use tarpc::util::Never;
use viper::VerificationResult;

service! {
    rpc verify(request: VerificationRequest) -> VerificationResult;
}

#[derive(Clone)]
pub struct ServerSideService {
    server: Arc<PrustiServer>,
}

impl ServerSideService {
    fn new(server: PrustiServer) -> Self {
        Self {
            server: Arc::new(server),
        }
    }

    pub fn spawn_off_thread() -> SocketAddr {
        let (sender, receiver) = mpsc::channel();
        thread::spawn(move || {
            let handle = ServerSideService::new(PrustiServer::new())
                .listen("localhost:0", server::Options::default())
                .unwrap();
            sender.send(handle.addr()).unwrap();
            handle.run();
        });
        receiver.recv().unwrap()
    }
}

impl SyncService for ServerSideService {
    fn verify(&self, request: VerificationRequest) -> Result<VerificationResult, Never> {
        Ok(self
            .server
            .run_verifier_async(request)
            .wait()
            .expect("verification task canceled—this should not be possible!"))
    }
}

pub struct PrustiServerConnection {
    client: SyncClient,
}

impl PrustiServerConnection {
    pub fn new_from_string(server_address: String) -> Self {
        Self::new(server_address.parse().unwrap())
    }

    pub fn new(server_address: SocketAddr) -> Self {
        let client = SyncClient::connect(server_address, client::Options::default()).unwrap();
        Self { client }
    }
}

impl VerificationService for PrustiServerConnection {
    fn verify(&self, request: VerificationRequest) -> VerificationResult {
        self.client.verify(request).unwrap()
    }
}