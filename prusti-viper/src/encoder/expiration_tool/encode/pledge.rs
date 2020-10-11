use prusti_common::vir;
use prusti_common::vir::Expr;
use prusti_interface::environment::mir_utils::AllPlacesForLocal;
use prusti_interface::specs::typed;
use rustc_middle::mir;

use crate::encoder::errors::ErrorCtxt;
use crate::encoder::expiration_tool::encode::ExpirationToolEncoder;
use crate::encoder::mir_encoder::PlaceEncoder;
use crate::encoder::places;

impl<'a, 'p, 'v: 'p, 'tcx: 'v> ExpirationToolEncoder<'a, 'p, 'v, 'tcx> {
    pub fn encode_pledge(&mut self, pledge: &typed::Assertion<'tcx>) -> vir::Expr {
        let mut encoded = self.encode_assertion(pledge);

        // Wrap arguments into `old[pre](...)`.
        encoded = self.procedure_encoder.wrap_arguments_into_old(
            encoded, self.pre_label, self.contract, &self.encoded_args);

        // All return places of the called function, with type information. These places are rooted
        // in _0, i.e., they are places of the called function, not the calling function.
        let return_places = mir::RETURN_PLACE.all_places_with_ty(self.tcx(), &self.called_mir());

        // All references returned by the function.
        let return_refs = return_places.into_iter().filter(|(_, ty)|
            self.procedure_encoder.mir_encoder.is_reference(ty)
        );

        // We now turn the mir::Places into places::Places (which can be interpreted in the
        // context of the calling function).
        let return_refs = if self.call_location.is_some() {
            return_refs.map(|(place, _)|
                places::Place::new_substituted(self.contract.returned_value, place)
            ).collect::<Vec<_>>()
        } else {
            return_refs.map(|(place, _)|
                places::Place::new_normal(place)
            ).collect::<Vec<_>>()
        };

        // Wrap return places into `old[post](...)`.
        for return_place in return_refs {
            let (encoded_return_place, ty, _) = self.procedure_encoder.encode_generic_place(
                self.def_id(), self.call_location, &return_place);
            let (encoded_return_place, _, _) = self.procedure_encoder.mir_encoder.encode_deref(
                encoded_return_place, ty);
            let encoded_old_return_place = vir::Expr::labelled_old(
                self.post_label, encoded_return_place.clone());
            encoded = encoded.replace_place(
                &encoded_return_place, &encoded_old_return_place);
        }

        encoded
    }

    fn encode_assertion(&self, assertion: &typed::Assertion<'tcx>) -> Expr {
        self.procedure_encoder.encoder.encode_assertion(
            assertion,
            self.procedure_encoder.mir,
            Some(self.pre_label),
            &self.encoded_args,
            Some(&self.encoded_return),
            false,
            None,
            ErrorCtxt::GenericExpression)
    }
}
