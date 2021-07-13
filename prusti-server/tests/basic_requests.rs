extern crate prusti_common;
extern crate prusti_server;
extern crate viper;
#[macro_use]
extern crate lazy_static;

use prusti_common::{
    verification_service::{VerificationRequest, VerificationService},
    vir::*,
};
use prusti_server::{PrustiServerConnection, ServerSideService};
use viper::ProgramVerificationResult;

lazy_static! {
    // only start the jvm & server once
    static ref SERVER_ADDRESS: String = ServerSideService::spawn_off_thread().to_string();
}

#[test]
fn consistency_error() {
    let result = process_program(|program| {
        program.fields.push(Field {
            name: "field".to_string(), // reserved identifier
            typ: Type::Int,
        });
    });

    let ProgramVerificationResult {
        verification_errors,
        mut consistency_errors,
        java_exceptions
    } = result;

    assert!(verification_errors.is_empty());
    assert!(java_exceptions.is_empty());
    let consistency_error = consistency_errors.pop().unwrap();
    assert!(consistency_errors.is_empty());
    println!("consistency error: {:?}", consistency_error);
}

#[test]
fn empty_program() {
    let result = process_program(|_| ());

    let ProgramVerificationResult {
        verification_errors,
        consistency_errors,
        java_exceptions
    } = result;

    assert!(verification_errors.is_empty());
    assert!(java_exceptions.is_empty());
    assert!(consistency_errors.is_empty());
}

fn process_program<F>(configure: F) -> ProgramVerificationResult
where
    F: FnOnce(&mut Program),
{
    let service =
        PrustiServerConnection::new(SERVER_ADDRESS.clone()).expect("Could not connect to server!");

    let mut program = Program {
        name: "very_dummy".to_string(),
        domains: vec![],
        fields: vec![],
        builtin_methods: vec![],
        methods: vec![],
        functions: vec![],
        viper_predicates: vec![],
    };
    configure(&mut program);

    let request = VerificationRequest {
        programs: vec![program],
        program_name: "dummy".to_string(),
        backend_config: Default::default(),
    };

    service.verify(request)
}
