// © 2020, ETH Zurich
//
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

use crate::encoder::mir_encoder::{MirEncoder, PlaceEncoder};
use crate::encoder::Encoder;
use prusti_common::vir;
use prusti_common::vir::Successor;
use prusti_common::config;
use prusti_interface::environment::Procedure;
use prusti_common::report::log;
use rustc_hir::def_id::DefId;
use rustc_middle::mir;
use ::log::trace;

pub struct StubProcedureEncoder<'p, 'v: 'p, 'tcx: 'v>
 {
    encoder: &'p Encoder<'v, 'tcx>,
    mir: &'p mir::Body<'tcx>,
    mir_encoder: MirEncoder<'p, 'v, 'tcx>,
    def_id: DefId,
    procedure: &'p Procedure<'tcx>,
}

impl<'p, 'v: 'p, 'tcx: 'v> StubProcedureEncoder<'p, 'v, 'tcx> {
    pub fn new(encoder: &'p Encoder<'v, 'tcx>, procedure: &'p Procedure<'tcx>) -> Self {
        let def_id = procedure.get_id();
        trace!("StubProcedureEncoder constructor: {:?}", def_id);

        let mir = procedure.get_mir();

        StubProcedureEncoder {
            encoder,
            mir,
            mir_encoder: MirEncoder::new(encoder, mir, def_id),
            def_id,
            procedure,
        }
    }

    pub fn encode(self) -> vir::CfgMethod {
        trace!("Encode stub for procedure {}", self.procedure.get_def_path());

        let mut cfg_method = vir::CfgMethod::new(
            // method name
            self.encoder.encode_item_name(self.def_id),
            // formal args
            self.mir.arg_count,
            // formal returns
            vec![],
            // local vars
            vec![],
            // reserved labels
            vec![],
        );

        // Declare the formal return
        for local in self.mir.local_decls.indices().take(1) {
            let name = self.mir_encoder.encode_local_var_name(local);
            let type_name = self
                .encoder
                .encode_type_predicate_use(self.mir_encoder.get_local_ty(local)).unwrap(); // will panic if attempting to encode unsupported type
            cfg_method.add_formal_return(&name, vir::Type::TypedRef(type_name))
        }

        // Initialize a single CFG block
        let stub_cfg_block = cfg_method.add_block(
            "stub",
            vec![
                vir::Stmt::comment("========== stub =========="),
                // vir::Stmt::comment(format!("Name: {:?}", self.procedure.get_name())),
                vir::Stmt::comment(format!("Def path: {:?}", self.procedure.get_def_path())),
                vir::Stmt::comment(format!("Span: {:?}", self.procedure.get_span())),
            ],
        );
        cfg_method.set_successor(stub_cfg_block, Successor::Return);

        // Dump method
        if config::dump_debug_info() {
            let method_name = cfg_method.name();
            let source_path = self.encoder.env().source_path();
            let source_filename = source_path.file_name().unwrap().to_str().unwrap();

            self.encoder
                .log_vir_program_before_foldunfold(cfg_method.to_string());

            log::report_with_writer(
                "graphviz_method_stub",
                format!("{}.{}.dot", source_filename, method_name),
                |writer| cfg_method.to_graphviz(writer),
            );
        }

        cfg_method
    }
}
