// © 2020, ETH Zurich
//
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

use super::{PrustiServer, RemoteVerificationResult};
use prusti_common::{config, verification_service::*};

use bincode;
use futures::{self};
use num_cpus;
use reqwest::{self, Client, Url, UrlError};
use std::{
    net::{Ipv4Addr, SocketAddr},
    sync::{mpsc, Arc},
    thread,
};
use tokio;
use viper::ProgramVerificationResult;
use warp::{self, Buf, Filter};

#[derive(Clone)]
pub struct ServerSideService {
    server: Arc<PrustiServer>,
    max_concurrency: usize,
}

impl ServerSideService {
    pub fn new() -> Self {
        // FIXME: since viper seems to dislike using verifiers in parallel, this is what we're doing to ensure correctness for now.
        // Eventually, we should lock only specific parts, instantiate multiple JVMs, or even address the root cause.
        let _max_concurrency = config::server_max_concurrency().unwrap_or_else(num_cpus::get);
        let max_concurrency = 1;

        let cache_size = config::server_max_stored_verifiers().unwrap_or(max_concurrency);
        if cache_size < max_concurrency {
            warn!("PRUSTI_SERVER_MAX_STORED_VERIFIERS is lower than PRUSTI_SERVER_MAX_CONCURRENCY—you probably don't want to do this, since it means the server will likely have to keep creating new verifiers, reducing the performance gained from reuse.");
        }

        Self {
            max_concurrency,
            server: Arc::new(PrustiServer::new(cache_size)),
        }
    }

    pub fn spawn_off_thread() -> SocketAddr {
        let (sender, receiver) = mpsc::channel();
        thread::spawn(move || {
            ServerSideService::new().listen_on_ephemeral_port(
                0, // ask system for port
                |address| sender.send(address).unwrap(),
            );
        });
        receiver.recv().unwrap()
    }

    pub fn listen_on_port(self, port: u16) {
        self.listen_on_ephemeral_port(port, |address| {
            if port == 0 {
                return;
            }
            assert_eq!(
                address.port(),
                port,
                "Server could not bind to port {}",
                port
            )
        });
    }

    fn listen_on_ephemeral_port<F>(self, port: u16, address_callback: F)
    where
        F: FnOnce(SocketAddr),
    {
        let clone = self.clone();
        let json_verify = warp::path("json")
            .and(warp::path("verify"))
            .and(warp::path::end())
            .and(warp::body::json())
            .map(move |request: VerificationRequest| clone.verify(request))
            .map(|response| warp::reply::json(&response));

        let clone = self.clone();
        let bincode_verify = warp::path("bincode")
            .and(warp::path("verify"))
            .and(warp::path::end())
            .and(warp::body::concat())
            .and_then(|buf: warp::body::FullBody| {
                bincode::deserialize(&buf.bytes()).map_err(|err| {
                    info!("request bincode body error: {}", err);
                    warp::reject::custom(err)
                })
            })
            .map(move |request: VerificationRequest| clone.verify(request))
            .map(|result| {
                warp::http::Response::new(
                    bincode::serialize(&result).expect("could not encode verification result"),
                )
            });

        let endpoints = json_verify.or(bincode_verify);

        info!("Prusti Server binding to port {}", port);
        let (address, server_handle) =
            warp::serve(endpoints).bind_ephemeral((Ipv4Addr::LOCALHOST, port));
        address_callback(address);

        info!("Prusti Server starting on port {}", address.port());
        let mut runtime = tokio::runtime::Builder::new()
            .name_prefix("prusti-server-")
            .core_threads(self.max_concurrency)
            .build()
            .expect("could not construct Tokio runtime!");

        runtime.spawn(futures::lazy(move || {
            warp::spawn(server_handle);
            info!("Prusti Server launched!");
            println!("port: {}", address.port()); // stdout, for use in other applications
            Ok(())
        }));

        thread::park();
    }

    fn verify(&self, request: VerificationRequest) -> RemoteVerificationResult {
        info!("Handling verification request for {}", request.program_name);
        self.server.run_verifier(request)
    }
}

pub struct PrustiServerConnection {
    client: Client,
    server_url: Url,
}

impl PrustiServerConnection {
    pub fn new<S: ToString>(server_address: S) -> Result<Self, UrlError> {
        let mut address = server_address.to_string();
        if !address.starts_with("http") {
            address = format!("http://{}", address);
        }
        Ok(Self {
            client: Client::builder().timeout(None).build().unwrap(),
            server_url: Url::parse(address.as_str())?,
        })
    }

    pub fn verify_checked(
        &self,
        request: VerificationRequest,
    ) -> reqwest::Result<RemoteVerificationResult> {
        let use_json = config::json_communication();
        let base = self.client.post(
            self.server_url
                .join(if use_json { "json/" } else { "bincode/" })
                .unwrap()
                .join("verify/")
                .unwrap(),
        );
        let response = if use_json {
            base.json(&request).send()?.error_for_status()?.json()?
        } else {
            let raw = base
                .body(bincode::serialize(&request).expect("error encoding verification request"))
                .send()?
                .error_for_status()?;
            bincode::deserialize_from(raw).expect("error decoding verification result")
        };
        Ok(response)
    }
}

impl VerificationService for PrustiServerConnection {
    /// panics if the verification request fails
    fn verify(&self, request: VerificationRequest) -> ProgramVerificationResult {
        self.verify_checked(request)
            .expect("Verification request to server failed!")
            .expect("Server panicked while processing request!")
    }
}
