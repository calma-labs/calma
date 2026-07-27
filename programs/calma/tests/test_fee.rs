mod common;
use common::{
    create_mint_ixs, create_token_account_ixs, mint_to_ix, read_token_balance, send_ixs,
    try_send_ixs,
};

use anchor_lang::prelude::Pubkey;
use anchor_lang::solana_program::clock::Clock;
use anchor_lang::solana_program::program_pack::Pack;
use calma::state::Pool;
use {
    anchor_lang::{solana_program::instruction::Instruction, InstructionData, ToAccountMetas},
    anchor_spl::token::spl_token,
    litesvm::LiteSVM,
    solana_keypair::Keypair,
    solana_signer::Signer,
};

const ATP_ID: &str = "ATokenGPvbdGVxr1b2hvZbsiqW5xWH25efTNsLJA8knL";

const LEND_DEPOSIT: u64 = 10_000_000;
const COL_DEPOSIT: u64 = 10_000_000;
const BORROW_AMOUNT: u64 = 5_000_000; // 50% utilization
const FEE_BPS: u64 = 2_000; // 20% of interest to the protocol
const YEAR: i64 = 31_557_600;

/// `pool.market.accrued_fee_shares` lives at Pool offset 192 (8-byte disc + 128
/// for four Pubkeys + market-relative 64), i.e. account-data offset 200.
fn read_accrued_fee_shares(svm: &LiteSVM, pool: &Pubkey) -> u64 {
    let acct = svm.get_account(pool).unwrap();
    u64::from_le_bytes(acct.data[200..208].try_into().unwrap())
}

/// Full flow proving the protocol fee wiring end-to-end: set a fee, let interest
/// accrue over a year, and confirm fee shares are minted and claimable as LP.
#[test]
fn test_protocol_fee_accrues_and_is_claimable() {
    let calma_id = calma::id();
    let irm_id = irm::id();
    let feed_id = feed::id();
    let atp_id: Pubkey = ATP_ID.parse().unwrap();

    let payer = Keypair::new();
    let col_mint_kp = Keypair::new();
    let lend_mint_kp = Keypair::new();
    let pool_kp = Keypair::new();
    let pool_pk = pool_kp.pubkey();

    let mut svm = LiteSVM::new();
    svm.add_program(calma_id, include_bytes!("../../../target/deploy/calma.so"))
        .unwrap();
    svm.add_program(irm_id, include_bytes!("../../../target/deploy/irm.so"))
        .unwrap();
    svm.add_program(feed_id, include_bytes!("../../../target/deploy/feed.so"))
        .unwrap();
    svm.airdrop(&payer.pubkey(), 200_000_000_000).unwrap();

    // A non-authority used to prove the fee instructions reject unauthorized callers.
    let intruder = Keypair::new();
    svm.airdrop(&intruder.pubkey(), 10_000_000_000).unwrap();

    // ── Mints ──────────────────────────────────────────────────────────────────
    let mint_rent = svm.minimum_balance_for_rent_exemption(spl_token::state::Mint::LEN);
    let [cc, ci] = create_mint_ixs(&payer.pubkey(), &col_mint_kp.pubkey(), &payer.pubkey(), mint_rent);
    let [lc, li] = create_mint_ixs(&payer.pubkey(), &lend_mint_kp.pubkey(), &payer.pubkey(), mint_rent);
    send_ixs(&mut svm, &[cc, ci, lc, li], &payer, &[&payer, &col_mint_kp, &lend_mint_kp]);

    let col_mint = col_mint_kp.pubkey();
    let lend_mint = lend_mint_kp.pubkey();

    // ── PDAs ───────────────────────────────────────────────────────────────────
    let (state_pda, _) = Pubkey::find_program_address(&[b"state"], &calma_id);
    let (col_vault, _) =
        Pubkey::find_program_address(&[b"collateral_vault", pool_pk.as_ref()], &calma_id);
    let (lend_vault, _) = Pubkey::find_program_address(&[b"lend_vault", pool_pk.as_ref()], &calma_id);
    let (lp_mint, _) = Pubkey::find_program_address(&[b"lp_mint", pool_pk.as_ref()], &calma_id);
    let (irm_config, _) = Pubkey::find_program_address(&[b"irm_config", pool_pk.as_ref()], &irm_id);
    let (user_position, _) = Pubkey::find_program_address(
        &[b"user_position", pool_pk.as_ref(), payer.pubkey().as_ref()],
        &calma_id,
    );
    let (user_lend_ata, _) = Pubkey::find_program_address(
        &[payer.pubkey().as_ref(), spl_token::id().as_ref(), lend_mint.as_ref()],
        &atp_id,
    );
    let (user_lp_ata, _) = Pubkey::find_program_address(
        &[payer.pubkey().as_ref(), spl_token::id().as_ref(), lp_mint.as_ref()],
        &atp_id,
    );

    // ── Feed ───────────────────────────────────────────────────────────────────
    let (feed_pda, _) = Pubkey::find_program_address(
        &[b"feed", col_mint.as_ref(), lend_mint.as_ref(), &[0u8]],
        &feed_id,
    );
    let feed_create_ix = Instruction::new_with_bytes(
        feed_id,
        &feed::instruction::Create {
            id: 0,
            source: feed::state::PriceSource::Manual,
            collateral_feed_id: [0u8; 32],
            lend_feed_id: [0u8; 32],
            rules: feed::state::FeedRules::default(),
        }
        .data(),
        feed::accounts::Create {
            feed: feed_pda,
            authority: payer.pubkey(),
            collateral_mint: col_mint,
            lend_mint,
            payer: payer.pubkey(),
            system_program: anchor_lang::solana_program::system_program::id(),
        }
        .to_account_metas(None),
    );

    // ── Pool alloc + IRM init + feed price ──────────────────────────────────────
    let pool_space = 8 + std::mem::size_of::<Pool>();
    let pool_rent = svm.minimum_balance_for_rent_exemption(pool_space);
    let alloc_pool_ix = anchor_lang::solana_program::system_instruction::create_account(
        &payer.pubkey(),
        &pool_pk,
        pool_rent,
        pool_space as u64,
        &calma_id,
    );
    let irm_init_ix = Instruction::new_with_bytes(
        irm_id,
        &irm::instruction::Initialize {
            points: vec![
                irm::RatePointArgs { util_bps: 0, rate_bps: 0 },
                irm::RatePointArgs { util_bps: 10_000, rate_bps: 500 },
            ],
        }
        .data(),
        irm::accounts::Initialize {
            irm_config,
            pool: pool_pk,
            authority: payer.pubkey(),
            payer: payer.pubkey(),
            system_program: anchor_lang::solana_program::system_program::id(),
        }
        .to_account_metas(None),
    );
    let feed_set_value_ix = Instruction::new_with_bytes(
        feed_id,
        &feed::instruction::SetValue { collateral_price: 1_000_000, lend_price: 1_000_000 }.data(),
        feed::accounts::SetValue { feed: feed_pda, authority: payer.pubkey() }.to_account_metas(None),
    );
    send_ixs(
        &mut svm,
        &[feed_create_ix, feed_set_value_ix, alloc_pool_ix, irm_init_ix],
        &payer,
        &[&payer, &pool_kp],
    );

    // ── Create pool ─────────────────────────────────────────────────────────────
    send_ixs(
        &mut svm,
        &[Instruction::new_with_bytes(
            calma_id,
            &calma::instruction::Create { ltv_percent: 75, max_feed_age_secs: 90u32 }.data(),
            calma::accounts::Create {
                pool: pool_pk,
                state: state_pda,
                collateral_vault: col_vault,
                lend_vault,
                lp_mint,
                collateral_mint: col_mint,
                lend_mint,
                authority: payer.pubkey(),
                payer: payer.pubkey(),
                feed_program: feed_id,
                feed_state: feed_pda,
                rate_program: irm_id,
                irm_state: irm_config,
                guard_program: None,
                guard_state: None,
                token_program: spl_token::id(),
                system_program: anchor_lang::solana_program::system_program::id(),
            }
            .to_account_metas(None),
        )],
        &payer,
        &[&payer],
    );

    // ── Set the protocol fee (authority-only) ───────────────────────────────────
    send_ixs(
        &mut svm,
        &[Instruction::new_with_bytes(
            calma_id,
            &calma::instruction::SetFee { fee_bps: FEE_BPS }.data(),
            calma::accounts::SetFee { pool: pool_pk, authority: payer.pubkey() }.to_account_metas(None),
        )],
        &payer,
        &[&payer],
    );

    // Guard: fee above MAX_FEE_BPS is rejected (FeeTooHigh).
    let over_cap = Instruction::new_with_bytes(
        calma_id,
        &calma::instruction::SetFee { fee_bps: 2_501 }.data(),
        calma::accounts::SetFee { pool: pool_pk, authority: payer.pubkey() }.to_account_metas(None),
    );
    assert!(!try_send_ixs(&mut svm, &[over_cap], &payer, &[&payer]), "fee over cap must be rejected");

    // Guard: a non-authority cannot set the fee (Unauthorized).
    let intruder_set = Instruction::new_with_bytes(
        calma_id,
        &calma::instruction::SetFee { fee_bps: 100 }.data(),
        calma::accounts::SetFee { pool: pool_pk, authority: intruder.pubkey() }.to_account_metas(None),
    );
    assert!(
        !try_send_ixs(&mut svm, &[intruder_set], &intruder, &[&intruder]),
        "non-authority set_fee must be rejected"
    );

    // ── User token accounts + funding ───────────────────────────────────────────
    let user_lend_src_kp = Keypair::new();
    let user_col_kp = Keypair::new();
    let ta_rent = svm.minimum_balance_for_rent_exemption(spl_token::state::Account::LEN);
    let [ulc, uli] = create_token_account_ixs(
        &payer.pubkey(), &user_lend_src_kp.pubkey(), &lend_mint, &payer.pubkey(), ta_rent,
    );
    let [ucc, uci] = create_token_account_ixs(
        &payer.pubkey(), &user_col_kp.pubkey(), &col_mint, &payer.pubkey(), ta_rent,
    );
    send_ixs(&mut svm, &[ulc, uli, ucc, uci], &payer, &[&payer, &user_lend_src_kp, &user_col_kp]);
    let user_lend_src = user_lend_src_kp.pubkey();
    let user_col_account = user_col_kp.pubkey();
    send_ixs(
        &mut svm,
        &[
            mint_to_ix(&lend_mint, &user_lend_src, &payer.pubkey(), LEND_DEPOSIT),
            mint_to_ix(&col_mint, &user_col_account, &payer.pubkey(), COL_DEPOSIT),
        ],
        &payer,
        &[&payer],
    );

    // ── Deposit lend, deposit collateral, borrow ────────────────────────────────
    send_ixs(
        &mut svm,
        &[Instruction::new_with_bytes(
            calma_id,
            &calma::instruction::DepositLent { amount: LEND_DEPOSIT }.data(),
            calma::accounts::DepositLent {
                pool: pool_pk,
                state: state_pda,
                lend_mint,
                lp_mint,
                authority: payer.pubkey(),
                user_lend_token_account: user_lend_src,
                user_lp_token_account: user_lp_ata,
                lend_vault,
                token_program: spl_token::id(),
                associated_token_program: atp_id,
                system_program: anchor_lang::solana_program::system_program::id(),
            }
            .to_account_metas(None),
        )],
        &payer,
        &[&payer],
    );
    send_ixs(
        &mut svm,
        &[Instruction::new_with_bytes(
            calma_id,
            &calma::instruction::DepositCollateral { amount: COL_DEPOSIT }.data(),
            calma::accounts::DepositCollateral {
                pool: pool_pk,
                collateral_mint: col_mint,
                authority: payer.pubkey(),
                user_token_account: user_col_account,
                collateral_vault: col_vault,
                user_position,
                token_program: spl_token::id(),
                system_program: anchor_lang::solana_program::system_program::id(),
            }
            .to_account_metas(None),
        )],
        &payer,
        &[&payer],
    );
    send_ixs(
        &mut svm,
        &[Instruction::new_with_bytes(
            calma_id,
            &calma::instruction::Borrow { amount: BORROW_AMOUNT }.data(),
            calma::accounts::Borrow {
                pool: pool_pk,
                state: state_pda,
                lend_mint,
                authority: payer.pubkey(),
                user_token_account: user_lend_ata,
                lend_vault,
                user_position,
                rate_program: irm_id,
                irm_state: irm_config,
                feed_program: feed_id,
                feed_state: feed_pda,
                token_program: spl_token::id(),
                associated_token_program: atp_id,
                system_program: anchor_lang::solana_program::system_program::id(),
            }
            .to_account_metas(None),
        )],
        &payer,
        &[&payer],
    );

    // No interest yet → no fee shares.
    assert_eq!(read_accrued_fee_shares(&svm, &pool_pk), 0);

    // Guard: claiming with nothing accrued is rejected (NoFeesToClaim).
    let empty_claim = Instruction::new_with_bytes(
        calma_id,
        &calma::instruction::ClaimFees {}.data(),
        calma::accounts::ClaimFees {
            pool: pool_pk,
            state: state_pda,
            lp_mint,
            authority: payer.pubkey(),
            authority_lp_token_account: user_lp_ata,
            token_program: spl_token::id(),
            associated_token_program: atp_id,
            system_program: anchor_lang::solana_program::system_program::id(),
        }
        .to_account_metas(None),
    );
    assert!(
        !try_send_ixs(&mut svm, &[empty_claim], &payer, &[&payer]),
        "claim with no accrued fees must be rejected"
    );

    // ── Advance one year and trigger accrual via repay ──────────────────────────
    let mut clock: Clock = svm.get_sysvar();
    clock.unix_timestamp += YEAR;
    svm.set_sysvar(&clock);

    // Fund the extra interest so a full repay can cover principal + interest.
    send_ixs(
        &mut svm,
        &[mint_to_ix(&lend_mint, &user_lend_ata, &payer.pubkey(), 1_000_000)],
        &payer,
        &[&payer],
    );
    send_ixs(
        &mut svm,
        &[Instruction::new_with_bytes(
            calma_id,
            &calma::instruction::Repay { amount: u64::MAX }.data(),
            calma::accounts::Repay {
                pool: pool_pk,
                lend_mint,
                authority: payer.pubkey(),
                user_token_account: user_lend_ata,
                lend_vault,
                user_position,
                rate_program: irm_id,
                irm_state: irm_config,
                token_program: spl_token::id(),
                associated_token_program: atp_id,
                system_program: anchor_lang::solana_program::system_program::id(),
            }
            .to_account_metas(None),
        )],
        &payer,
        &[&payer],
    );

    // Interest accrued → protocol fee shares were minted.
    let fee_shares = read_accrued_fee_shares(&svm, &pool_pk);
    assert!(fee_shares > 0, "expected protocol fee shares to accrue");

    // Guard: a non-authority cannot claim the fee (Unauthorized), even with
    // accrued shares available.
    let (intruder_lp_ata, _) = Pubkey::find_program_address(
        &[intruder.pubkey().as_ref(), spl_token::id().as_ref(), lp_mint.as_ref()],
        &atp_id,
    );
    let intruder_claim = Instruction::new_with_bytes(
        calma_id,
        &calma::instruction::ClaimFees {}.data(),
        calma::accounts::ClaimFees {
            pool: pool_pk,
            state: state_pda,
            lp_mint,
            authority: intruder.pubkey(),
            authority_lp_token_account: intruder_lp_ata,
            token_program: spl_token::id(),
            associated_token_program: atp_id,
            system_program: anchor_lang::solana_program::system_program::id(),
        }
        .to_account_metas(None),
    );
    assert!(
        !try_send_ixs(&mut svm, &[intruder_claim], &intruder, &[&intruder]),
        "non-authority claim_fees must be rejected"
    );
    // The failed claim must not have touched the accrued counter.
    assert_eq!(read_accrued_fee_shares(&svm, &pool_pk), fee_shares);

    // ── Claim fees → authority receives matching LP, counter resets ─────────────
    let lp_before = read_token_balance(&svm, &user_lp_ata);
    // Distinct blockhash: the successful claim is otherwise byte-identical to the
    // earlier NoFeesToClaim probe and would collide as AlreadyProcessed.
    svm.expire_blockhash();
    send_ixs(
        &mut svm,
        &[Instruction::new_with_bytes(
            calma_id,
            &calma::instruction::ClaimFees {}.data(),
            calma::accounts::ClaimFees {
                pool: pool_pk,
                state: state_pda,
                lp_mint,
                authority: payer.pubkey(),
                authority_lp_token_account: user_lp_ata,
                token_program: spl_token::id(),
                associated_token_program: atp_id,
                system_program: anchor_lang::solana_program::system_program::id(),
            }
            .to_account_metas(None),
        )],
        &payer,
        &[&payer],
    );

    let lp_after = read_token_balance(&svm, &user_lp_ata);
    assert_eq!(lp_after - lp_before, fee_shares, "authority receives fee shares as LP");
    assert_eq!(read_accrued_fee_shares(&svm, &pool_pk), 0, "counter reset after claim");
}
