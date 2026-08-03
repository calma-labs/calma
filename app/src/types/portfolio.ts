export interface LendPosition {
  id: string;
  asset: string;
  icon: string;
  collateralAsset: string;
  collateralIcon: string;
  supplied: number;
  apy: number;
  earned: number;
  health: number;
  collateralEnabled: boolean;
  /** Raw exact amount as string to avoid floating point precision issues when withdrawing max */
  rawSupplied?: string;
}

export interface BorrowPosition {
  id: string;
  poolId: string;
  collateralAsset: string;
  collateralIcon: string;
  collateralAmount: number;
  borrowedAsset: string;
  borrowedIcon: string;
  debtAmount: number;
  borrowAPY: number;
  supplyAPY: number;
  ltv: number | null;
  liqPrice: number | null;
  healthFactor: number | null;
}

export interface MultiplyPosition {
  id: string;
  poolId: string;
  asset: string;
  icon: string;
  debtAsset: string;
  debtIcon: string;
  multiplier: number;
  netAPY: number;
  positionSize: number;
  /** `null` when unknown, for the same reason as `pnl` below. */
  entryPrice: number | null;
  currentPrice: number | null;
  liqPrice: number | null;
  /** `null` when PnL cannot be computed — see the note on `UserPosition` below.
   *
   * A position's PnL needs the collateral price at the time it was opened, and
   * `UserPosition` (crates/state) stores only `collateral_deposited` and
   * `debt_shares`. Nothing on-chain records an entry price and there is no
   * historical index, so for now this is genuinely unavailable rather than zero.
   * Rendering it as `$0 (0.00%)` reads as "you are exactly break-even", which is
   * a claim the data does not support. */
  pnl: number | null;
  pnlPct: number | null;
}

export interface PortfolioHistoryPoint {
  date: string;
  value: number;
}

export interface PortfolioSummary {
  netValue: number;
  totalSupplied: number;
  totalDebt: number;
  leveragedExposure: number;
  change30d: number;
  change30dPct: number;
  history: PortfolioHistoryPoint[];
}
