use crate::{mock::*, Error, 
	DesiredCandidates, ProposedCandidates, CandidateInfo
};
use pallet_collator_selection::Invulnerables;
use frame_support::{
	assert_noop, assert_ok, traits::Hooks
};
use pallet_session::SessionManager;

#[test]
fn it_works_for_default_value() {
	new_test_ext().execute_with(|| {
		// Check the desired candidates from runtime config
		System::set_block_number(1);
		XodeStaking::on_initialize(1);
		let desired_candidates = DesiredCandidates::<Test>::get();
		assert_eq!(desired_candidates.len(), 3, "There should be exactly three desired candidates");
		
		// Todo: Check the invulnerabilities (current authors)
		XodeStaking::new_session(1);
		assert_eq!(Invulnerables::<Test>::get(), desired_candidates, "The invulnerables should match the desired candidates");

		// Todo: Add candidates (at least 2 candidates) [Candidate-1, Candidate-2]
		let _ = XodeStaking::register_candidate(RuntimeOrigin::signed(1));
		let _ = XodeStaking::register_candidate(RuntimeOrigin::signed(2));
		let mut candidate_1 = CandidateInfo {
			who: 1,
			bond: 0,
			total_stake: 0,
			last_updated: 1,
			leaving: false,
			offline: false,
			commission: 0,
		};
		let mut candidate_2 = CandidateInfo {
			who: 2,
			bond: 0,
			total_stake: 0,
			last_updated: 1,
			leaving: false,
			offline: false,
			commission: 0,
		};
		let proposed_candidates = ProposedCandidates::<Test>::get();
		assert_eq!(proposed_candidates.len(), 2, "The number of proposed candidates should be 2");
		assert_eq!(proposed_candidates[0], candidate_1, "The first candidate data does not match");
		assert_eq!(proposed_candidates[1], candidate_2, "The second candidate data does not match");

		// Todo: Bond the candidate
		// Note:
		//      1. Sorted proposed canidates
		//      2. Last updated block number
		System::set_block_number(2);
		let _ = XodeStaking::bond_candidate(RuntimeOrigin::signed(1), 100);
		let _ = XodeStaking::bond_candidate(RuntimeOrigin::signed(2), 200);

		candidate_1.bond = 100;
		candidate_1.last_updated = 2;
		candidate_2.bond = 200;
		candidate_2.last_updated = 2;
		let proposed_candidates = ProposedCandidates::<Test>::get();
		assert_eq!(proposed_candidates[0], candidate_2, "The first candidate data does not match");
		assert_eq!(proposed_candidates[1], candidate_1, "The second candidate data does not match");

		// Todo: Increase the bond of a candidate
		// Note:
		//      1. Sorted proposed canidates
		//      2. Check if the bond amount is correct
		let _ = XodeStaking::bond_candidate(RuntimeOrigin::signed(1), 400);

		candidate_1.bond = 400;
		let proposed_candidates = ProposedCandidates::<Test>::get();
		assert_eq!(proposed_candidates[0], candidate_1, "The first candidate data does not match");
		assert_eq!(proposed_candidates[1], candidate_2, "The second candidate data does not match");


		// Todo: Decrease the bond of a candidate
		// Note:
		//      1. Sorted proposed canidates (using last updated since bond amount is equal)
		//      2. Check if the bond amount is correct
		System::set_block_number(3);
		let _ = XodeStaking::bond_candidate(RuntimeOrigin::signed(1), 200);

		candidate_1.bond = 200;
		candidate_1.last_updated = 3;
		let proposed_candidates = ProposedCandidates::<Test>::get();
		assert_eq!(proposed_candidates[0], candidate_2, "The first candidate data does not match");
		assert_eq!(proposed_candidates[1], candidate_1, "The second candidate data does not match");

		// Todo: Move to the next session

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

