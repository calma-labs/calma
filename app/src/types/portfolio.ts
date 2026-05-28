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
  entryPrice: number;
  currentPrice: number;
  liqPrice: number | null;
  pnl: number;
  pnlPct: number;
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
