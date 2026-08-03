//! LiteSVM coverage for the source-aware feed program: cross-source gating,
//! the Pyth pull path (`set_from_pyth`), and staleness rejection.

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

const COLL_FEED_ID: [u8; 32] = [0x11u8; 32];
const LEND_FEED_ID: [u8; 32] = [0x22u8; 32];
const MAX_AGE_MS: u32 = 60_000;

/// Creates a fresh SPL Token Mint via the built-in token program LiteSVM
/// preloads. Returns the mint pubkey. `payer` funds account creation and
/// signs as mint authority.
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

/// Fabricates a Pyth `PriceUpdateV2` account directly in the LiteSVM ledger so
/// the on-chain `Account<'info, PriceUpdateV2>` deserializer (owner +
/// discriminator + Borsh body) accepts it. `Full` verification is used so the
/// SDK's `get_price_no_older_than` (which demands `Full`) doesn't reject the
/// fixture before the staleness check runs.
fn write_pyth_price_update(
    svm: &mut LiteSVM,
    key: Pubkey,
    feed_id: [u8; 32],
    price: i64,
    exponent: i32,
    publish_time: i64,
) {
    write_pyth_price_update_full(svm, key, feed_id, price, price, 0, exponent, publish_time);
}

/// Full-control variant that lets rule tests set `conf` and `ema_price`
/// explicitly.
fn write_pyth_price_update_full(
    svm: &mut LiteSVM,
    key: Pubkey,
    feed_id: [u8; 32],
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
            feed_id,
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
    // Default decimals 6 ↔ ratio formula reduces to coll_price/lend_price × PRICE_SCALE.
    let collateral_mint = create_mint(&mut svm, &payer, 6);
    let lend_mint = create_mint(&mut svm, &payer, 6);
    Ctx {
        svm,
        payer,
        collateral_mint,
        lend_mint,
    }
}

fn feed_pda(collateral_mint: &Pubkey, lend_mint: &Pubkey) -> Pubkey {
    Pubkey::find_program_address(
        &[
            b"feed",
            collateral_mint.as_ref(),
            lend_mint.as_ref(),
            &[0u8],
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
) -> Instruction {
    let max_age_ms = if matches!(source, PriceSource::Pyth) { MAX_AGE_MS } else { 0 };
    create_ix_with_rules(
        payer,
        authority,
        collateral_mint,
        lend_mint,
        source,
        coll_feed_id,
        lend_feed_id,
        FeedRules { max_age_ms, ..Default::default() },
    )
}

fn create_ix_with_rules(
    payer: &Pubkey,
    authority: &Pubkey,
    collateral_mint: Pubkey,
    lend_mint: Pubkey,
    source: PriceSource,
    coll_feed_id: [u8; 32],
    lend_feed_id: [u8; 32],
    rules: FeedRules,
) -> Instruction {
    Instruction::new_with_bytes(
        feed::id(),
        &feed::instruction::Create {
            id: 0,
            source,
            collateral_feed_id: coll_feed_id,
            lend_feed_id,
            price_ttl_ms: 90_000,
            rules,
        }
        .data(),
        feed::accounts::Create {
            feed: feed_pda(&collateral_mint, &lend_mint),
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
            feed: feed_pda(collateral_mint, lend_mint),
            authority: *authority,
        }
        .to_account_metas(None),
    )
}

fn set_from_pyth_ix(
    collateral_mint: &Pubkey,
    lend_mint: &Pubkey,
    coll_update: Pubkey,
    lend_update: Pubkey,
) -> Instruction {
    Instruction::new_with_bytes(
        feed::id(),
        &feed::instruction::SetFromPyth {}.data(),
        feed::accounts::SetFromPyth {
            feed: feed_pda(collateral_mint, lend_mint),
            collateral_price_update: coll_update,
            lend_price_update: lend_update,
        }
        .to_account_metas(None),
    )
}

// ── Tests ────────────────────────────────────────────────────────────────────

#[test]
fn manual_create_and_set_value_happy_path() {
    let mut ctx = fresh_svm();
    assert!(send_ixs(
        &mut ctx.svm,
        &[create_ix(
            &ctx.payer.pubkey(),
            &ctx.payer.pubkey(),
            ctx.collateral_mint,
            ctx.lend_mint,
            PriceSource::Manual,
            [0u8; 32],
            [0u8; 32],
        )],
        &ctx.payer,
    ));
    assert!(send_ixs(
        &mut ctx.svm,
        &[set_value_ix(&ctx.payer.pubkey(), &ctx.collateral_mint, &ctx.lend_mint, 1_000_000, 1_000_000)],
        &ctx.payer,
    ));
    // Both mints in fresh_svm are decimals=6 → feed inherits them.
    let feed_account = ctx.svm.get_account(&feed_pda(&ctx.collateral_mint, &ctx.lend_mint)).unwrap();
    let feed = Feed::try_deserialize(&mut feed_account.data.as_slice()).unwrap();
    assert_eq!(feed.header.collateral_decimals, 6);
    assert_eq!(feed.header.lend_decimals, 6);
    assert_eq!(feed.header.collateral_mint, ctx.collateral_mint);
    assert_eq!(feed.header.lend_mint, ctx.lend_mint);
}

#[test]
fn pyth_create_with_zero_feed_ids_rejected() {
    let mut ctx = fresh_svm();
    assert!(!send_ixs(
        &mut ctx.svm,
        &[create_ix(
            &ctx.payer.pubkey(),
            &ctx.payer.pubkey(),
            ctx.collateral_mint,
            ctx.lend_mint,
            PriceSource::Pyth,
            [0u8; 32], // invalid: Pyth source requires non-zero feed IDs
            [0u8; 32],
        )],
        &ctx.payer,
    ));
}

#[test]
fn manual_create_with_nonzero_feed_ids_rejected() {
    let mut ctx = fresh_svm();
    assert!(!send_ixs(
        &mut ctx.svm,
        &[create_ix(
            &ctx.payer.pubkey(),
            &ctx.payer.pubkey(),
            ctx.collateral_mint,
            ctx.lend_mint,
            PriceSource::Manual,
            COLL_FEED_ID, // invalid: Manual source requires zeroed feed IDs
            LEND_FEED_ID,
        )],
        &ctx.payer,
    ));
}

#[test]
fn manual_feed_rejects_set_from_pyth() {
    let mut ctx = fresh_svm();
    assert!(send_ixs(
        &mut ctx.svm,
        &[create_ix(
            &ctx.payer.pubkey(),
            &ctx.payer.pubkey(),
            ctx.collateral_mint,
            ctx.lend_mint,
            PriceSource::Manual,
            [0u8; 32],
            [0u8; 32],
        )],
        &ctx.payer,
    ));

    // Even with valid PriceUpdateV2 fixtures, a Manual feed must reject
    // set_from_pyth.
    let coll_pk = Pubkey::new_unique();
    let lend_pk = Pubkey::new_unique();
    write_pyth_price_update(&mut ctx.svm, coll_pk, [0u8; 32], 1, -6, 1_000);
    write_pyth_price_update(&mut ctx.svm, lend_pk, [0u8; 32], 1, -6, 1_000);

    assert!(!send_ixs(
        &mut ctx.svm,
        &[set_from_pyth_ix(&ctx.collateral_mint, &ctx.lend_mint, coll_pk, lend_pk)],
        &ctx.payer,
    ));
}

#[test]
fn pyth_feed_rejects_set_value() {
    let mut ctx = fresh_svm();
    assert!(send_ixs(
        &mut ctx.svm,
        &[create_ix(
            &ctx.payer.pubkey(),
            &ctx.payer.pubkey(),
            ctx.collateral_mint,
            ctx.lend_mint,
            PriceSource::Pyth,
            COLL_FEED_ID,
            LEND_FEED_ID,
        )],
        &ctx.payer,
    ));
    assert!(!send_ixs(
        &mut ctx.svm,
        &[set_value_ix(&ctx.payer.pubkey(), &ctx.collateral_mint, &ctx.lend_mint, 1_000_000, 1_000_000)],
        &ctx.payer,
    ));
}

#[test]
fn pyth_set_from_pyth_happy_path() {
    let mut ctx = fresh_svm();
    // Pin the on-chain clock so the freshness check is deterministic.
    let now: i64 = 1_700_000_000;
    ctx.svm.set_sysvar::<Clock>(&Clock {
        unix_timestamp: now,
        ..Default::default()
    });

    assert!(send_ixs(
        &mut ctx.svm,
        &[create_ix(
            &ctx.payer.pubkey(),
            &ctx.payer.pubkey(),
            ctx.collateral_mint,
            ctx.lend_mint,
            PriceSource::Pyth,
            COLL_FEED_ID,
            LEND_FEED_ID,
        )],
        &ctx.payer,
    ));

    let coll_pk = Pubkey::new_unique();
    let lend_pk = Pubkey::new_unique();
    // Pyth-style: price = 12_345, exponent = -2 → 123.45 in real terms;
    // normalized to PRICE_SCALE (1e6) → 123_450_000.
    write_pyth_price_update(&mut ctx.svm, coll_pk, COLL_FEED_ID, 12_345, -2, now - 10);
    // lend at parity (1.0 in PRICE_SCALE) so the ratio reduces cleanly.
    write_pyth_price_update(&mut ctx.svm, lend_pk, LEND_FEED_ID, 1_000_000, -6, now - 5);

    assert!(send_ixs(
        &mut ctx.svm,
        &[set_from_pyth_ix(&ctx.collateral_mint, &ctx.lend_mint, coll_pk, lend_pk)],
        &ctx.payer,
    ));

    let feed_account = ctx.svm.get_account(&feed_pda(&ctx.collateral_mint, &ctx.lend_mint)).unwrap();
    let feed = Feed::try_deserialize(&mut feed_account.data.as_slice()).unwrap();
    assert_eq!(feed.header.collateral_price, 123_450_000);
    assert_eq!(feed.header.lend_price, 1_000_000);
    // last_updated_ts == min(publish_time)
    assert_eq!(feed.header.last_updated_ts, now - 10);
}

/// Common setup for rule tests: pin the clock, create a Pyth-source feed with
/// the given rules, and return the fixture context plus the pinned timestamp.
fn setup_pyth_feed_with_rules(rules: FeedRules) -> (Ctx, i64) {
    let rules = FeedRules { max_age_ms: MAX_AGE_MS, ..rules };
    let mut ctx = fresh_svm();
    let now: i64 = 1_700_000_000;
    ctx.svm.set_sysvar::<Clock>(&Clock {
        unix_timestamp: now,
        ..Default::default()
    });
    assert!(send_ixs(
        &mut ctx.svm,
        &[create_ix_with_rules(
            &ctx.payer.pubkey(),
            &ctx.payer.pubkey(),
            ctx.collateral_mint,
            ctx.lend_mint,
            PriceSource::Pyth,
            COLL_FEED_ID,
            LEND_FEED_ID,
            rules,
        )],
        &ctx.payer,
    ));
    (ctx, now)
}

#[test]
fn pyth_confidence_within_max_conf_bps_accepts() {
    // 100 conf on 1_000_000 price = 1 bps → equal to the cap.
    let rules = FeedRules {
        max_conf_bps: 1,
        ..Default::default()
    };
    let (mut ctx, now) = setup_pyth_feed_with_rules(rules);
    let coll_pk = Pubkey::new_unique();
    let lend_pk = Pubkey::new_unique();
    write_pyth_price_update_full(
        &mut ctx.svm, coll_pk, COLL_FEED_ID, 1_000_000, 1_000_000, 100, -6, now,
    );
    write_pyth_price_update_full(
        &mut ctx.svm, lend_pk, LEND_FEED_ID, 1_000_000, 1_000_000, 100, -6, now,
    );
    assert!(send_ixs(
        &mut ctx.svm,
        &[set_from_pyth_ix(&ctx.collateral_mint, &ctx.lend_mint, coll_pk, lend_pk)],
        &ctx.payer,
    ));
}

#[test]
fn pyth_confidence_over_max_conf_bps_rejects() {
    let rules = FeedRules {
        max_conf_bps: 1,
        ..Default::default()
    };
    let (mut ctx, now) = setup_pyth_feed_with_rules(rules);
    let coll_pk = Pubkey::new_unique();
    let lend_pk = Pubkey::new_unique();
    // 101 / 1_000_000 > 1 bps on the collateral leg.
    write_pyth_price_update_full(
        &mut ctx.svm, coll_pk, COLL_FEED_ID, 1_000_000, 1_000_000, 101, -6, now,
    );
    write_pyth_price_update_full(
        &mut ctx.svm, lend_pk, LEND_FEED_ID, 1_000_000, 1_000_000, 0, -6, now,
    );
    assert!(!send_ixs(
        &mut ctx.svm,
        &[set_from_pyth_ix(&ctx.collateral_mint, &ctx.lend_mint, coll_pk, lend_pk)],
        &ctx.payer,
    ));
}

#[test]
fn pyth_price_below_min_price_rejects() {
    // min = 1.5e6 (normalized). A 1.0 price normalizes to 1_000_000 → below floor.
    let rules = FeedRules {
        min_price: 1_500_000,
        ..Default::default()
    };
    let (mut ctx, now) = setup_pyth_feed_with_rules(rules);
    let coll_pk = Pubkey::new_unique();
    let lend_pk = Pubkey::new_unique();
    write_pyth_price_update_full(
        &mut ctx.svm, coll_pk, COLL_FEED_ID, 1_000_000, 1_000_000, 0, -6, now,
    );
    write_pyth_price_update_full(
        &mut ctx.svm, lend_pk, LEND_FEED_ID, 2_000_000, 2_000_000, 0, -6, now,
    );
    assert!(!send_ixs(
        &mut ctx.svm,
        &[set_from_pyth_ix(&ctx.collateral_mint, &ctx.lend_mint, coll_pk, lend_pk)],
        &ctx.payer,
    ));
}

#[test]
fn pyth_price_above_max_price_rejects() {
    let rules = FeedRules {
        max_price: 1_500_000,
        ..Default::default()
    };
    let (mut ctx, now) = setup_pyth_feed_with_rules(rules);
    let coll_pk = Pubkey::new_unique();
    let lend_pk = Pubkey::new_unique();
    // Lend leg exceeds the ceiling.
    write_pyth_price_update_full(
        &mut ctx.svm, coll_pk, COLL_FEED_ID, 1_000_000, 1_000_000, 0, -6, now,
    );
    write_pyth_price_update_full(
        &mut ctx.svm, lend_pk, LEND_FEED_ID, 2_000_000, 2_000_000, 0, -6, now,
    );
    assert!(!send_ixs(
        &mut ctx.svm,
        &[set_from_pyth_ix(&ctx.collateral_mint, &ctx.lend_mint, coll_pk, lend_pk)],
        &ctx.payer,
    ));
}

#[test]
fn pyth_ema_divergence_over_budget_rejects() {
    // Allow up to 500 bps (5%) between spot and EMA.
    let rules = FeedRules {
        ema_divergence_bps: 500,
        ..Default::default()
    };
    let (mut ctx, now) = setup_pyth_feed_with_rules(rules);
    let coll_pk = Pubkey::new_unique();
    let lend_pk = Pubkey::new_unique();
    // spot = 1_060_000 vs ema = 1_000_000 → 600 bps > 500 → reject.
    write_pyth_price_update_full(
        &mut ctx.svm, coll_pk, COLL_FEED_ID, 1_060_000, 1_000_000, 0, -6, now,
    );
    write_pyth_price_update_full(
        &mut ctx.svm, lend_pk, LEND_FEED_ID, 1_000_000, 1_000_000, 0, -6, now,
    );
    assert!(!send_ixs(
        &mut ctx.svm,
        &[set_from_pyth_ix(&ctx.collateral_mint, &ctx.lend_mint, coll_pk, lend_pk)],
        &ctx.payer,
    ));
}

#[test]
fn pyth_first_update_skips_deviation_check() {
    // The tightest budget `validate_rules` will accept. Without the
    // first-update skip, seeding any price at all against a stored 0 is an
    // unbounded relative move and would be rejected; because last_updated_ts is
    // 0 the check is a no-op.
    let rules = FeedRules {
        max_deviation_bps_per_hour: feed::state::MIN_DEVIATION_BPS_PER_HOUR,
        ..Default::default()
    };
    let (mut ctx, now) = setup_pyth_feed_with_rules(rules);
    let coll_pk = Pubkey::new_unique();
    let lend_pk = Pubkey::new_unique();
    write_pyth_price_update_full(
        &mut ctx.svm, coll_pk, COLL_FEED_ID, 1_000_000, 1_000_000, 0, -6, now,
    );
    write_pyth_price_update_full(
        &mut ctx.svm, lend_pk, LEND_FEED_ID, 1_000_000, 1_000_000, 0, -6, now,
    );
    assert!(send_ixs(
        &mut ctx.svm,
        &[set_from_pyth_ix(&ctx.collateral_mint, &ctx.lend_mint, coll_pk, lend_pk)],
        &ctx.payer,
    ));
}

#[test]
fn pyth_deviation_budget_scales_with_elapsed_time() {
    // 1000 bps/hour (the floor `validate_rules` enforces) means a 2000 bps
    // jump is rejected after 1 hour but accepted after 2.
    let rules = FeedRules {
        max_deviation_bps_per_hour: feed::state::MIN_DEVIATION_BPS_PER_HOUR,
        ..Default::default()
    };
    let (mut ctx, now) = setup_pyth_feed_with_rules(rules);
    let coll_pk = Pubkey::new_unique();
    let lend_pk = Pubkey::new_unique();

    // First update seeds the state.
    write_pyth_price_update_full(
        &mut ctx.svm, coll_pk, COLL_FEED_ID, 1_000_000, 1_000_000, 0, -6, now,
    );
    write_pyth_price_update_full(
        &mut ctx.svm, lend_pk, LEND_FEED_ID, 1_000_000, 1_000_000, 0, -6, now,
    );
    assert!(send_ixs(
        &mut ctx.svm,
        &[set_from_pyth_ix(&ctx.collateral_mint, &ctx.lend_mint, coll_pk, lend_pk)],
        &ctx.payer,
    ));

    // 1 hour later: try a +2000 bps move on the collateral leg → over budget.
    let one_hour_later = now + 3_600;
    ctx.svm.set_sysvar::<Clock>(&Clock {
        unix_timestamp: one_hour_later,
        ..Default::default()
    });
    ctx.svm.expire_blockhash();
    write_pyth_price_update_full(
        &mut ctx.svm,
        coll_pk,
        COLL_FEED_ID,
        1_200_000, // +2000 bps vs 1_000_000
        1_200_000,
        0,
        -6,
        one_hour_later,
    );
    write_pyth_price_update_full(
        &mut ctx.svm, lend_pk, LEND_FEED_ID, 1_000_000, 1_000_000, 0, -6, one_hour_later,
    );
    assert!(!send_ixs(
        &mut ctx.svm,
        &[set_from_pyth_ix(&ctx.collateral_mint, &ctx.lend_mint, coll_pk, lend_pk)],
        &ctx.payer,
    ));

    // Push the clock to 2 hours after the seed update → 2000 bps now fits.
    let two_hours_later = now + 7_200;
    ctx.svm.set_sysvar::<Clock>(&Clock {
        unix_timestamp: two_hours_later,
        ..Default::default()
    });
    ctx.svm.expire_blockhash();
    write_pyth_price_update_full(
        &mut ctx.svm,
        coll_pk,
        COLL_FEED_ID,
        1_200_000,
        1_200_000,
        0,
        -6,
        two_hours_later,
    );
    write_pyth_price_update_full(
        &mut ctx.svm, lend_pk, LEND_FEED_ID, 1_000_000, 1_000_000, 0, -6, two_hours_later,
    );
    assert!(send_ixs(
        &mut ctx.svm,
        &[set_from_pyth_ix(&ctx.collateral_mint, &ctx.lend_mint, coll_pk, lend_pk)],
        &ctx.payer,
    ));
}

#[test]
fn pyth_create_rejects_invalid_rules_min_greater_than_max() {
    let mut ctx = fresh_svm();
    let rules = FeedRules {
        min_price: 2_000_000,
        max_price: 1_000_000,
        ..Default::default()
    };
    assert!(!send_ixs(
        &mut ctx.svm,
        &[create_ix_with_rules(
            &ctx.payer.pubkey(),
            &ctx.payer.pubkey(),
            ctx.collateral_mint,
            ctx.lend_mint,
            PriceSource::Pyth,
            COLL_FEED_ID,
            LEND_FEED_ID,
            rules,
        )],
        &ctx.payer,
    ));
}

#[test]
fn pyth_set_from_pyth_rejects_stale_update() {
    let mut ctx = fresh_svm();
    let now: i64 = 1_700_000_000;
    ctx.svm.set_sysvar::<Clock>(&Clock {
        unix_timestamp: now,
        ..Default::default()
    });

    assert!(send_ixs(
        &mut ctx.svm,
        &[create_ix(
            &ctx.payer.pubkey(),
            &ctx.payer.pubkey(),
            ctx.collateral_mint,
            ctx.lend_mint,
            PriceSource::Pyth,
            COLL_FEED_ID,
            LEND_FEED_ID,
        )],
        &ctx.payer,
    ));

    let coll_pk = Pubkey::new_unique();
    let lend_pk = Pubkey::new_unique();
    // Collateral well past MAX_AGE_MS (60_000 ms) → SDK returns PriceTooOld.
    write_pyth_price_update(&mut ctx.svm, coll_pk, COLL_FEED_ID, 12_345, -2, now - 1_000);
    write_pyth_price_update(&mut ctx.svm, lend_pk, LEND_FEED_ID, 1_000_000, -6, now);

    assert!(!send_ixs(
        &mut ctx.svm,
        &[set_from_pyth_ix(&ctx.collateral_mint, &ctx.lend_mint, coll_pk, lend_pk)],
        &ctx.payer,
    ));
}
