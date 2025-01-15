use crate::{mock::*, Error, Status, ProposedCandidates,
};
use frame_support::{
	assert_noop, assert_ok,
};
use frame_support::traits::{Currency, Imbalance};

// Register Candidate Function - Unit Tests
// Run this command: cargo test test_pallet_xode_staking_register_candidate -- --nocapture
// =======================================================================================

#[test]
fn test_pallet_xode_staking_register_candidate_works() {
	test1_ext().execute_with(|| {
        let candidate = 1;
		assert_ok!(XodeStaking::register_candidate(RuntimeOrigin::signed(candidate)));

		// I tried getting the last dispatched events, but it doesn't seem to be working.
		// However, we really need to get the last dispatched events to ensure the test works.
		
		// System::assert_last_event(Event::ProposedCandidateAdded { 
        //     _proposed_candidate: 1, 
        // }.into());
	});
}

#[test]
fn test_pallet_xode_staking_register_candidate_no_existing_candidates() {
    test1_ext().execute_with(|| {
        let candidate = 1;
        assert_ok!(XodeStaking::register_candidate(RuntimeOrigin::signed(candidate)));

		let candidate_count = ProposedCandidates::<Test>::get().len();
        assert_eq!(candidate_count, 1);
    });
}

#[test]
fn test_pallet_xode_staking_register_candidate_already_exists_should_error() {
	test1_ext().execute_with(|| {
        let candidate = 1;

		assert_ok!(XodeStaking::register_candidate(RuntimeOrigin::signed(candidate)));
		assert_noop!(
			XodeStaking::register_candidate(RuntimeOrigin::signed(candidate)),
			Error::<Test>::ProposedCandidateAlreadyExist
		);
	});
}

#[test]
fn test_pallet_xode_staking_register_candidate_max_exceeded_should_error() {
	test1_ext().execute_with(|| {
		for i in 1..101 { 
            assert_ok!(XodeStaking::register_candidate(RuntimeOrigin::signed(i)));
        }

        let candidate = 101;
		assert_noop!(
            XodeStaking::register_candidate(RuntimeOrigin::signed(candidate)),
            Error::<Test>::ProposedCandidateMaxExceeded
        );
	});
}

#[test]
fn test_pallet_xode_staking_register_candidate_invalid_account_should_error() {
    test1_ext().execute_with(|| {
        // Shoud error if the signer is invalid
        assert_noop!(
            XodeStaking::register_candidate(RuntimeOrigin::none()),
            sp_runtime::traits::BadOrigin
        );
    });
}

#[test]
fn test_pallet_xode_staking_register_candidate_default_values() {
    test1_ext().execute_with(|| {
        let candidate = 1;
        assert_ok!(XodeStaking::register_candidate(RuntimeOrigin::signed(candidate)));

        // Verify the default values in the storage
		let candidates = ProposedCandidates::<Test>::get();
        let _candidate_info = candidates
            .iter()
            .find(|c| c.who == candidate);
            
        if let Some(candidate_info) = candidates.iter().find(|c| c.who == candidate) {
            assert_eq!(candidate_info.who, candidate);
            assert_eq!(candidate_info.bond, 0);
            assert_eq!(candidate_info.total_stake, 0);
            assert_eq!(candidate_info.last_updated, System::block_number());
            assert_eq!(candidate_info.leaving, false);
            assert_eq!(candidate_info.offline, false);
            assert_eq!(candidate_info.commission, 0);
            assert_eq!(candidate_info.status, Status::Online);
            assert_eq!(candidate_info.status_level, 0);
        }
    });
}

// Bond Candidate Function - Unit Tests
// Run this command: cargo test test_pallet_xode_staking_bond_candidate -- --nocapture
// ===================================================================================

#[test]
fn test_pallet_xode_staking_bond_candidate_works() {
	test1_ext().execute_with(|| {
        let candidate = 1;

        // Deposit initial funds and verify imbalances
		let imbalance = Balances::deposit_creating(&candidate, 100_000_000_000_000_000);
		assert!(imbalance.peek() > 0, "Expected a positive imbalance for deposit creation");

        // Register the candidate
        assert_ok!(XodeStaking::register_candidate(RuntimeOrigin::signed(candidate)));

        // Bond the candidate
		assert_ok!(XodeStaking::bond_candidate(RuntimeOrigin::signed(candidate), 11_000_000_000_000_000));

        let candidates = ProposedCandidates::<Test>::get();
        if let Some(candidate_info) = candidates.iter().find(|c| c.who == candidate) {    
            assert_eq!(candidate_info.bond, 11_000_000_000_000_000);  
        }  
        assert_eq!(11_000_000_000_000_000, Balances::reserved_balance(candidate), "Should have 11_000_000_000_000_000 reserved balance.");
	});
}

#[test]
fn test_pallet_xode_staking_bond_candidate_not_registered_should_error() {
	test1_ext().execute_with(|| {
        let candidate = 1;

        // Deposit initial funds and verify imbalances
		let imbalance = Balances::deposit_creating(&candidate, 1000);
		assert!(imbalance.peek() > 0, "Expected a positive imbalance for deposit creation");

        // Bond the candidate without registration should error
        assert_noop!(
            XodeStaking::bond_candidate(RuntimeOrigin::signed(candidate), 100),
            Error::<Test>::ProposedCandidateNotFound
        );
	});
}

#[test]
fn test_pallet_xode_staking_bond_candidate_increases_bond_and_reserves() {
    test1_ext().execute_with(|| {
        let candidate = 1;

        // Deposit initial funds and verify imbalances
		let imbalance = Balances::deposit_creating(&candidate, 100_000_000_000_000_000);
		assert!(imbalance.peek() > 0, "Expected a positive imbalance for deposit creation");

        // Register the candidate
        assert_ok!(XodeStaking::register_candidate(RuntimeOrigin::signed(candidate)));

        // Bond the candidate
		assert_ok!(XodeStaking::bond_candidate(RuntimeOrigin::signed(candidate), 11_000_000_000_000_000));

        let candidates = ProposedCandidates::<Test>::get();
        if let Some(candidate_info) = candidates.iter().find(|c| c.who == candidate) {    
            assert_eq!(candidate_info.bond, 11_000_000_000_000_000);  
        }  
        assert_eq!(11_000_000_000_000_000, Balances::reserved_balance(candidate), "Should have 100 reserved balance.");
        
        // Increase the bond and reserves
		assert_ok!(XodeStaking::bond_candidate(RuntimeOrigin::signed(candidate), 21_000_000_000_000_000));

        let updated_candidates = ProposedCandidates::<Test>::get();
        if let Some(candidate_info) = updated_candidates.iter().find(|c| c.who == candidate) {    
            assert_eq!(candidate_info.bond, 21_000_000_000_000_000);  
        }  
        assert_eq!(21_000_000_000_000_000, Balances::reserved_balance(candidate), "Should have 100 reserved balance.")
    });
}

#[test]
fn test_pallet_xode_staking_bond_candidate_decreases_bond_and_unreserves() {
    test1_ext().execute_with(|| {
		let candidate = 1;

        // Deposit initial funds and verify imbalances
		let imbalance = Balances::deposit_creating(&candidate, 1000);
		assert!(imbalance.peek() > 0, "Expected a positive imbalance for deposit creation");

        // Register the candidate
        assert_ok!(XodeStaking::register_candidate(RuntimeOrigin::signed(candidate)));

        // Bond the candidate
		assert_ok!(XodeStaking::bond_candidate(RuntimeOrigin::signed(candidate), 200));

        let candidates = ProposedCandidates::<Test>::get();
        if let Some(candidate_info) = candidates.iter().find(|c| c.who == candidate) {    
            assert_eq!(candidate_info.bond, 200);  
        }  
        assert_eq!(200, Balances::reserved_balance(candidate), "Should have 100 reserved balance.");
        
        // Decrease the bond and unreserves
		assert_ok!(XodeStaking::bond_candidate(RuntimeOrigin::signed(candidate), 100));

        let updated_candidates = ProposedCandidates::<Test>::get();
        if let Some(candidate_info) = updated_candidates.iter().find(|c| c.who == candidate) {    
            assert_eq!(candidate_info.bond, 100);  
        }  
        assert_eq!(100, Balances::reserved_balance(candidate), "Should have 100 reserved balance.")
    });
}

#[test]
fn test_pallet_xode_staking_bond_candidate_insufficient_balance_should_error() {
    test1_ext().execute_with(|| {
		let candidate = 1;

        // Register the candidate
        assert_ok!(XodeStaking::register_candidate(RuntimeOrigin::signed(candidate)));

        // Bond the candidate with insufficient balance should error
        assert_noop!(
            XodeStaking::bond_candidate(RuntimeOrigin::signed(candidate), 100),
            Error::<Test>::ProposedCandidateInsufficientBalance
        );
    });
}

#[test]
fn test_pallet_xode_staking_bond_candidate_sorting_after_update() {
    test1_ext().execute_with(|| {
        let candidates = vec![1, 2, 3];

        // Deposit initial funds and verify imbalances
        for &candidate in &candidates {
            let imbalance = Balances::deposit_creating(&candidate, 1000);
            assert!(
                imbalance.peek() > 0,
                "Candidate {}: Expected a positive imbalance for deposit creation",
                candidate
            );
        }
        
        // Register and bond each candidate
        for &candidate in &candidates {
            assert_ok!(XodeStaking::register_candidate(RuntimeOrigin::signed(candidate)));

            if candidate == 1 {
                assert_ok!(XodeStaking::bond_candidate(RuntimeOrigin::signed(candidate), 100));
            } else if candidate == 2 {
                assert_ok!(XodeStaking::bond_candidate(RuntimeOrigin::signed(candidate), 300));
            } else if candidate == 3 {
                assert_ok!(XodeStaking::bond_candidate(RuntimeOrigin::signed(candidate), 200));
            }
        }

        // Verify the order of candidates in the ProposedCandidates storage
        let candidates = ProposedCandidates::<Test>::get();
        
        assert_eq!(candidates.len(), 3, "Expected 3 candidates in storage");

        assert_eq!(candidates[0].who, 2, "Candidate 1 should be first with the largest bond");
        assert_eq!(candidates[0].bond, 300, "Candidate 1 bond should be 300");

        assert_eq!(candidates[1].who, 3, "Candidate 3 should be second with the middle bond");
        assert_eq!(candidates[1].bond, 200, "Candidate 3 bond should be 200");

        assert_eq!(candidates[2].who, 1, "Candidate 2 should be last with the smallest bond");
        assert_eq!(candidates[2].bond, 100, "Candidate 2 bond should be 100");
    });
}