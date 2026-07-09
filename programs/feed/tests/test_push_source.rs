//! LiteSVM coverage for the sponsored Pyth push feed path (`set_from_pyth_push`).
//! Push accounts share the `PriceUpdateV2` layout with the pull flow; only the
//! validation model differs — we pin by account pubkey stored in the feed
//! config rather than by Pyth feed_id hash.

use {
    anchor_lang::{
        prelude::Pubkey,
        solana_program::{instruction::Instruction, program_pack::Pack, system_instruction},
        AccountDeserialize, AnchorSerialize, Discriminator, InstructionData, ToAccountMetas,
    },
    anchor_spl::token::spl_token::{self, state::Mint as SplMint},
    feed::state::{Feed, FeedRules, PriceSource},
    litesvm::LiteSVM,
    pyth_solana_receiver_sdk::price_update::{PriceFeedMessage, PriceUpdateV2, VerificationLevel},
    solana_account::Account,
    solana_clock::Clock,
    solana_keypair::Keypair,
    solana_message::{Message, VersionedMessage},
    solana_signer::Signer,
    solana_transaction::versioned::VersionedTransaction,
};

const FEED_SO: &[u8] = include_bytes!("../../../target/deploy/feed.so");

const MAX_AGE_SECS: u32 = 60;

fn create_mint(svm: &mut LiteSVM, payer: &Keypair, decimals: u8) -> Pubkey {
    let mint_kp = Keypair::new();
    let rent = svm.minimum_balance_for_rent_exemption(SplMint::LEN);
    let ixs = [
        system_instruction::create_account(
            &payer.pubkey(),
            &mint_kp.pubkey(),
            rent,
            SplMint::LEN as u64,
            &spl_token::id(),
        ),
        spl_token::instruction::initialize_mint(
            &spl_token::id(),
            &mint_kp.pubkey(),
            &payer.pubkey(),
            None,
            decimals,
        )
        .unwrap(),
    ];
    let bh = svm.latest_blockhash();
    let msg = Message::new_with_blockhash(&ixs, Some(&payer.pubkey()), &bh);
    let tx = VersionedTransaction::try_new(VersionedMessage::Legacy(msg), &[payer, &mint_kp])
        .unwrap();
    svm.send_transaction(tx).expect("create mint");
    mint_kp.pubkey()
}

/// Writes a sponsored-style `PriceUpdateV2` at `key`. The account is
/// owner-checked against `pyth_solana_receiver_sdk::ID` by Anchor's
/// `Account<'info, PriceUpdateV2>` deserializer, exactly like the pull path.
/// `feed_id` here is the Pyth internal feed_id — irrelevant to `set_from_pyth_push`,
/// which pins on the account's *pubkey*, not this field.
fn write_push_update(
    svm: &mut LiteSVM,
    key: Pubkey,
    price: i64,
    ema_price: i64,
    conf: u64,
    exponent: i32,
    publish_time: i64,
) {
    let update = PriceUpdateV2 {
        write_authority: Pubkey::default(),
        verification_level: VerificationLevel::Full,
        price_message: PriceFeedMessage {
            feed_id: [0xAA; 32],
            price,
            conf,
            exponent,
            publish_time,
            prev_publish_time: publish_time.saturating_sub(1),
            ema_price,
            ema_conf: 0,
        },
        posted_slot: 0,
    };

    let mut data = Vec::with_capacity(PriceUpdateV2::LEN);
    data.extend_from_slice(&PriceUpdateV2::DISCRIMINATOR);
    update.serialize(&mut data).expect("borsh serialize");

    let lamports = svm
        .minimum_balance_for_rent_exemption(data.len())
        .max(1_000_000);
    svm.set_account(
        key,
        Account {
            lamports,
            data,
            owner: pyth_solana_receiver_sdk::ID,
            executable: false,
            rent_epoch: 0,
        },
    )
    .expect("set price update account");
}

fn send_ixs(svm: &mut LiteSVM, ixs: &[Instruction], payer: &Keypair) -> bool {
    let bh = svm.latest_blockhash();
    let msg = Message::new_with_blockhash(ixs, Some(&payer.pubkey()), &bh);
    let tx = VersionedTransaction::try_new(VersionedMessage::Legacy(msg), &[payer]).unwrap();
    svm.send_transaction(tx).is_ok()
}

struct Ctx {
    svm: LiteSVM,
    payer: Keypair,
    collateral_mint: Pubkey,
    lend_mint: Pubkey,
}

fn fresh_svm() -> Ctx {
    let mut svm = LiteSVM::new();
    svm.add_program(feed::id(), FEED_SO)
        .expect("load feed.so — run `anchor build` first");
    let payer = Keypair::new();
    svm.airdrop(&payer.pubkey(), 10_000_000_000).unwrap();
    let collateral_mint = create_mint(&mut svm, &payer, 6);
    let lend_mint = create_mint(&mut svm, &payer, 6);
    Ctx {
        svm,
        payer,
        collateral_mint,
        lend_mint,
    }
}

fn feed_pda(authority: &Pubkey, collateral_mint: &Pubkey, lend_mint: &Pubkey) -> Pubkey {
    Pubkey::find_program_address(
        &[
            b"feed",
            authority.as_ref(),
            collateral_mint.as_ref(),
            lend_mint.as_ref(),
        ],
        &feed::id(),
    )
    .0
}

fn create_ix(
    payer: &Pubkey,
    authority: &Pubkey,
    collateral_mint: Pubkey,
    lend_mint: Pubkey,
    source: PriceSource,
    coll_feed_id: [u8; 32],
    lend_feed_id: [u8; 32],
    max_pyth_age_secs: u32,
    rules: FeedRules,
) -> Instruction {
    Instruction::new_with_bytes(
        feed::id(),
        &feed::instruction::Create {
            source,
            collateral_feed_id: coll_feed_id,
            lend_feed_id,
            max_pyth_age_secs,
            rules,
        }
        .data(),
        feed::accounts::Create {
            feed: feed_pda(authority, &collateral_mint, &lend_mint),
            authority: *authority,
            collateral_mint,
            lend_mint,
            payer: *payer,
            system_program: anchor_lang::solana_program::system_program::id(),
        }
        .to_account_metas(None),
    )
}

fn set_value_ix(
    authority: &Pubkey,
    collateral_mint: &Pubkey,
    lend_mint: &Pubkey,
    collateral_price: u64,
    lend_price: u64,
) -> Instruction {
    Instruction::new_with_bytes(
        feed::id(),
        &feed::instruction::SetValue {
            collateral_price,
            lend_price,
        }
        .data(),
        feed::accounts::SetValue {
            feed: feed_pda(authority, collateral_mint, lend_mint),
            authority: *authority,
        }
        .to_account_metas(None),
    )
}

fn set_from_pyth_ix(
    authority: &Pubkey,
    collateral_mint: &Pubkey,
    lend_mint: &Pubkey,
    coll_update: Pubkey,
    lend_update: Pubkey,
) -> Instruction {
    Instruction::new_with_bytes(
        feed::id(),
        &feed::instruction::SetFromPyth {}.data(),
        feed::accounts::SetFromPyth {
            feed: feed_pda(authority, collateral_mint, lend_mint),
            collateral_price_update: coll_update,
            lend_price_update: lend_update,
        }
        .to_account_metas(None),
    )
}

fn set_from_pyth_push_ix(
    authority: &Pubkey,
    collateral_mint: &Pubkey,
    lend_mint: &Pubkey,
    coll_update: Pubkey,
    lend_update: Pubkey,
) -> Instruction {
    Instruction::new_with_bytes(
        feed::id(),
        &feed::instruction::SetFromPythPush {}.data(),
        feed::accounts::SetFromPythPush {
            feed: feed_pda(authority, collateral_mint, lend_mint),
            collateral_price_update: coll_update,
            lend_price_update: lend_update,
        }
        .to_account_metas(None),
    )
}

/// Convenience: create a `PythPush` feed pinned to `(coll_pk, lend_pk)` with
/// the given rules and clock, and return the pinned pubkeys.
fn setup_push_feed(rules: FeedRules) -> (Ctx, Pubkey, Pubkey, i64) {
    let mut ctx = fresh_svm();
    let now: i64 = 1_700_000_000;
    ctx.svm.set_sysvar::<Clock>(&Clock {
        unix_timestamp: now,
        ..Default::default()
    });
    let coll_pk = Pubkey::new_unique();
    let lend_pk = Pubkey::new_unique();
    assert!(send_ixs(
        &mut ctx.svm,
        &[create_ix(
            &ctx.payer.pubkey(),
            &ctx.payer.pubkey(),
            ctx.collateral_mint,
            ctx.lend_mint,
            PriceSource::PythPush,
            coll_pk.to_bytes(),
            lend_pk.to_bytes(),
            MAX_AGE_SECS,
            rules,
        )],
        &ctx.payer,
    ));
    (ctx, coll_pk, lend_pk, now)
}

// ── Tests ────────────────────────────────────────────────────────────────────

#[test]
fn push_create_with_zero_feed_ids_rejected() {
    let mut ctx = fresh_svm();
    assert!(!send_ixs(
        &mut ctx.svm,
        &[create_ix(
            &ctx.payer.pubkey(),
            &ctx.payer.pubkey(),
            ctx.collateral_mint,
            ctx.lend_mint,
            PriceSource::PythPush,
            [0u8; 32],
            [0u8; 32],
            MAX_AGE_SECS,
            FeedRules::default(),
        )],
        &ctx.payer,
    ));
}

#[test]
fn push_create_with_zero_max_age_rejected() {
    let mut ctx = fresh_svm();
    let coll_pk = Pubkey::new_unique();
    let lend_pk = Pubkey::new_unique();
    assert!(!send_ixs(
        &mut ctx.svm,
        &[create_ix(
            &ctx.payer.pubkey(),
            &ctx.payer.pubkey(),
            ctx.collateral_mint,
            ctx.lend_mint,
            PriceSource::PythPush,
            coll_pk.to_bytes(),
            lend_pk.to_bytes(),
            0,
            FeedRules::default(),
        )],
        &ctx.payer,
    ));
}

#[test]
fn push_feed_rejects_set_value() {
    let (mut ctx, _coll_pk, _lend_pk, _now) = setup_push_feed(FeedRules::default());
    assert!(!send_ixs(
        &mut ctx.svm,
        &[set_value_ix(&ctx.payer.pubkey(), &ctx.collateral_mint, &ctx.lend_mint, 1_000_000, 1_000_000)],
        &ctx.payer,
    ));
}

#[test]
fn push_feed_rejects_set_from_pyth() {
    let (mut ctx, coll_pk, lend_pk, now) = setup_push_feed(FeedRules::default());
    write_push_update(&mut ctx.svm, coll_pk, 1_000_000, 1_000_000, 0, -6, now);
    write_push_update(&mut ctx.svm, lend_pk, 1_000_000, 1_000_000, 0, -6, now);
    // Source gate should reject before Pyth feed_id matching runs.
    assert!(!send_ixs(
        &mut ctx.svm,
        &[set_from_pyth_ix(&ctx.payer.pubkey(), &ctx.collateral_mint, &ctx.lend_mint, coll_pk, lend_pk)],
        &ctx.payer,
    ));
}

#[test]
fn push_set_from_pyth_push_happy_path() {
    let (mut ctx, coll_pk, lend_pk, now) = setup_push_feed(FeedRules::default());
    // 12_345 × 10^-2 = 123.45 → normalized to 1e6 scale = 123_450_000.
    write_push_update(&mut ctx.svm, coll_pk, 12_345, 12_345, 0, -2, now - 10);
    write_push_update(&mut ctx.svm, lend_pk, 1_000_000, 1_000_000, 0, -6, now - 5);
    assert!(send_ixs(
        &mut ctx.svm,
        &[set_from_pyth_push_ix(&ctx.payer.pubkey(), &ctx.collateral_mint, &ctx.lend_mint, coll_pk, lend_pk)],
        &ctx.payer,
    ));
    let feed_account = ctx.svm.get_account(&feed_pda(&ctx.payer.pubkey(), &ctx.collateral_mint, &ctx.lend_mint)).unwrap();
    let feed = Feed::try_deserialize(&mut feed_account.data.as_slice()).unwrap();
    assert_eq!(feed.state.collateral_price, 123_450_000);
    assert_eq!(feed.state.lend_price, 1_000_000);
    assert_eq!(feed.state.last_updated_ts, now - 10);
}

#[test]
fn push_wrong_account_pubkey_rejected() {
    let (mut ctx, coll_pk, lend_pk, now) = setup_push_feed(FeedRules::default());
    let bogus = Pubkey::new_unique();
    // `bogus` is a valid PriceUpdateV2 fixture but not the pinned collateral pubkey.
    write_push_update(&mut ctx.svm, bogus, 1_000_000, 1_000_000, 0, -6, now);
    write_push_update(&mut ctx.svm, lend_pk, 1_000_000, 1_000_000, 0, -6, now);
    assert!(!send_ixs(
        &mut ctx.svm,
        &[set_from_pyth_push_ix(&ctx.payer.pubkey(), &ctx.collateral_mint, &ctx.lend_mint, bogus, lend_pk)],
        &ctx.payer,
    ));
    // Sanity: the *right* pubkey pair is still accepted.
    write_push_update(&mut ctx.svm, coll_pk, 1_000_000, 1_000_000, 0, -6, now);
    assert!(send_ixs(
        &mut ctx.svm,
        &[set_from_pyth_push_ix(&ctx.payer.pubkey(), &ctx.collateral_mint, &ctx.lend_mint, coll_pk, lend_pk)],
        &ctx.payer,
    ));
}

#[test]
fn push_stale_price_rejected() {
    let (mut ctx, coll_pk, lend_pk, now) = setup_push_feed(FeedRules::default());
    // Collateral publish_time well past MAX_AGE_SECS (60s).
    write_push_update(&mut ctx.svm, coll_pk, 1_000_000, 1_000_000, 0, -6, now - 1_000);
    write_push_update(&mut ctx.svm, lend_pk, 1_000_000, 1_000_000, 0, -6, now);
    assert!(!send_ixs(
        &mut ctx.svm,
        &[set_from_pyth_push_ix(&ctx.payer.pubkey(), &ctx.collateral_mint, &ctx.lend_mint, coll_pk, lend_pk)],
        &ctx.payer,
    ));
}

#[test]
fn push_confidence_over_max_conf_bps_rejects() {
    let rules = FeedRules {
        max_conf_bps: 1,
        ..Default::default()
    };
    let (mut ctx, coll_pk, lend_pk, now) = setup_push_feed(rules);
    // 101 / 1_000_000 > 1 bps → reject.
    write_push_update(&mut ctx.svm, coll_pk, 1_000_000, 1_000_000, 101, -6, now);
    write_push_update(&mut ctx.svm, lend_pk, 1_000_000, 1_000_000, 0, -6, now);
    assert!(!send_ixs(
        &mut ctx.svm,
        &[set_from_pyth_push_ix(&ctx.payer.pubkey(), &ctx.collateral_mint, &ctx.lend_mint, coll_pk, lend_pk)],
        &ctx.payer,
    ));
}

#[test]
fn push_price_out_of_bounds_rejects() {
    let rules = FeedRules {
        min_price: 1_500_000,
        ..Default::default()
    };
    let (mut ctx, coll_pk, lend_pk, now) = setup_push_feed(rules);
    // 1.0 normalized (1_000_000) is below the 1.5 floor.
    write_push_update(&mut ctx.svm, coll_pk, 1_000_000, 1_000_000, 0, -6, now);
    write_push_update(&mut ctx.svm, lend_pk, 2_000_000, 2_000_000, 0, -6, now);
    assert!(!send_ixs(
        &mut ctx.svm,
        &[set_from_pyth_push_ix(&ctx.payer.pubkey(), &ctx.collateral_mint, &ctx.lend_mint, coll_pk, lend_pk)],
        &ctx.payer,
    ));
}

#[test]
fn push_ema_divergence_over_budget_rejects() {
    let rules = FeedRules {
        ema_divergence_bps: 500,
        ..Default::default()
    };
    let (mut ctx, coll_pk, lend_pk, now) = setup_push_feed(rules);
    // spot 1_060_000 vs ema 1_000_000 → 600 bps > 500 → reject.
    write_push_update(&mut ctx.svm, coll_pk, 1_060_000, 1_000_000, 0, -6, now);
    write_push_update(&mut ctx.svm, lend_pk, 1_000_000, 1_000_000, 0, -6, now);
    assert!(!send_ixs(
        &mut ctx.svm,
        &[set_from_pyth_push_ix(&ctx.payer.pubkey(), &ctx.collateral_mint, &ctx.lend_mint, coll_pk, lend_pk)],
        &ctx.payer,
    ));
}

#[test]
fn push_deviation_budget_enforced_after_first_update() {
    let rules = FeedRules {
        max_deviation_bps_per_hour: 100,
        ..Default::default()
    };
    let (mut ctx, coll_pk, lend_pk, now) = setup_push_feed(rules);
    // Seed the state.
    write_push_update(&mut ctx.svm, coll_pk, 1_000_000, 1_000_000, 0, -6, now);
    write_push_update(&mut ctx.svm, lend_pk, 1_000_000, 1_000_000, 0, -6, now);
    assert!(send_ixs(
        &mut ctx.svm,
        &[set_from_pyth_push_ix(&ctx.payer.pubkey(), &ctx.collateral_mint, &ctx.lend_mint, coll_pk, lend_pk)],
        &ctx.payer,
    ));

    // 1 hour later: 200 bps jump on collateral exceeds the 100 bps/hour budget.
    let one_hour_later = now + 3_600;
    ctx.svm.set_sysvar::<Clock>(&Clock {
        unix_timestamp: one_hour_later,
        ..Default::default()
    });
    ctx.svm.expire_blockhash();
    write_push_update(&mut ctx.svm, coll_pk, 1_020_000, 1_020_000, 0, -6, one_hour_later);
    write_push_update(&mut ctx.svm, lend_pk, 1_000_000, 1_000_000, 0, -6, one_hour_later);
    assert!(!send_ixs(
        &mut ctx.svm,
        &[set_from_pyth_push_ix(&ctx.payer.pubkey(), &ctx.collateral_mint, &ctx.lend_mint, coll_pk, lend_pk)],
        &ctx.payer,
    ));
}
