use crate::{mock::*, Error, DesiredCandidates};
use frame_support::{assert_noop, assert_ok};

#[test]
fn it_works_for_default_value() {
	new_test_ext().execute_with(|| {
		let desirec_candidates = DesiredCandidates::<Test>::get();
		assert_eq!(desirec_candidates.len(), 2, "There should be exactly three desired candidates");
	});
}

