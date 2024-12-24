use crate::{mock::*, Error, DesiredCandidates};
use frame_support::{
	assert_noop, assert_ok, traits::Hooks
};

#[test]
fn it_works_for_default_value() {
	new_test_ext().execute_with(|| {
		// Check the desired candidates from runtime config
		System::set_block_number(1);
		XodeStaking::on_initialize(1);
		let desirec_candidates = DesiredCandidates::<Test>::get();
		assert_eq!(desirec_candidates.len(), 3, "There should be exactly three desired candidates");
		
		// Todo: Check the invulnerabilities (current authors)

		// Todo: Add candidates (at least 2 candidates) [Candidate-A, Candidate-B]

		// Todo: Bond the candidate

		// Todo: Increase the bond of a candidate

		// Todo: Add stakes (at lease 3 stakes per candidate)

		// Todo: Unstake 1 delegator per candidate

		// Todo: Make one candidate to go offline [Candidate-B]

		// Todo: Make the offline candidate to go online [Candidate-B]

		// Todo: Add another candidate (do not bond) [Candidate-D]

		// Todo: Add another candidate then bond [Candidate-E]

		// Todo: Stake the new candidate using the delegators of the first candidate [Candidate-E, Candidate-A(first candidate)]

		// Todo: Set the first candidate to leaving [Candidate-A]

		// Todo: Now that only Candidate-B and Candidate-E are online, let them author a block

		// Todo: Candidate-B did not author a block
	});
}

