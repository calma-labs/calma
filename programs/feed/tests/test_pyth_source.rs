//! LiteSVM coverage for the source-aware feed program: cross-source gating,
//! the Pyth pull path (`set_from_pyth`), and staleness rejection.

use {
    anchor_lang::{
        prelude::Pubkey,
        solana_program::{instruction::Instruction, program_pack::Pack, system_instruction},
        AnchorSerialize, Discriminator, InstructionData, ToAccountMetas,
    },
    anchor_spl::token::spl_token::{self, state::Mint as SplMint},
    feed::state::PriceSource,
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
    let update = PriceUpdateV2 {
        write_authority: Pubkey::default(),
        verification_level: VerificationLevel::Full,
        price_message: PriceFeedMessage {
            feed_id,
            price,
            conf: 0,
            exponent,
            publish_time,
            prev_publish_time: publish_time.saturating_sub(1),
            ema_price: price,
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

fn feed_pda(authority: &Pubkey) -> Pubkey {
    Pubkey::find_program_address(&[b"feed", authority.as_ref()], &feed::id()).0
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
    // For Pyth-source feeds the param is required; for Manual it's ignored.
    let max_pyth_age_secs = if matches!(source, PriceSource::Pyth) {
        60
    } else {
        0
    };
    Instruction::new_with_bytes(
        feed::id(),
        &feed::instruction::Create {
            source,
            collateral_feed_id: coll_feed_id,
            lend_feed_id,
            max_pyth_age_secs,
        }
        .data(),
        feed::accounts::Create {
            feed: feed_pda(authority),
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
            feed: feed_pda(authority),
            authority: *authority,
        }
        .to_account_metas(None),
    )
}

fn set_from_pyth_ix(
    authority: &Pubkey,
    coll_update: Pubkey,
    lend_update: Pubkey,
) -> Instruction {
    Instruction::new_with_bytes(
        feed::id(),
        &feed::instruction::SetFromPyth {}.data(),
        feed::accounts::SetFromPyth {
            feed: feed_pda(authority),
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
        &[set_value_ix(&ctx.payer.pubkey(), 1_000_000, 1_000_000)],
        &ctx.payer,
    ));
    // Both mints in fresh_svm are decimals=6 → feed inherits them.
    let feed_account = ctx.svm.get_account(&feed_pda(&ctx.payer.pubkey())).unwrap();
    let body = &feed_account.data[8..];
    assert_eq!(body[34], 6); // collateral_decimals
    assert_eq!(body[35], 6); // lend_decimals
    // Mints are stored at offsets 64..96 and 96..128.
    assert_eq!(&body[64..96], ctx.collateral_mint.as_ref());
    assert_eq!(&body[96..128], ctx.lend_mint.as_ref());
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
        &[set_from_pyth_ix(&ctx.payer.pubkey(), coll_pk, lend_pk)],
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
        &[set_value_ix(&ctx.payer.pubkey(), 1_000_000, 1_000_000)],
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
        &[set_from_pyth_ix(&ctx.payer.pubkey(), coll_pk, lend_pk)],
        &ctx.payer,
    ));

    let feed_account = ctx.svm.get_account(&feed_pda(&ctx.payer.pubkey())).unwrap();
    // Skip the 8-byte Anchor discriminator and parse the fields directly.
    let body = &feed_account.data[8..];
    // Layout: authority(32) source(1) bump(1) coll_dec(1) lend_dec(1) pad(4)
    //         coll_price(8) lend_price(8) last_updated_ts(8)
    //         coll_mint(32) lend_mint(32) coll_feed_id(32) lend_feed_id(32) reserved(32)
    let coll_price = u64::from_le_bytes(body[40..48].try_into().unwrap());
    let lend_price = u64::from_le_bytes(body[48..56].try_into().unwrap());
    let last_ts = i64::from_le_bytes(body[56..64].try_into().unwrap());
    assert_eq!(coll_price, 123_450_000);
    assert_eq!(lend_price, 1_000_000);
    // last_updated_ts == min(publish_time)
    assert_eq!(last_ts, now - 10);
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
    // Collateral well past MAX_PYTH_AGE_SECS (60s) → SDK returns PriceTooOld.
    write_pyth_price_update(&mut ctx.svm, coll_pk, COLL_FEED_ID, 12_345, -2, now - 1_000);
    write_pyth_price_update(&mut ctx.svm, lend_pk, LEND_FEED_ID, 1_000_000, -6, now);

    assert!(!send_ixs(
        &mut ctx.svm,
        &[set_from_pyth_ix(&ctx.payer.pubkey(), coll_pk, lend_pk)],
        &ctx.payer,
    ));
}
