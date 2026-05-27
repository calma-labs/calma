import type { PoolWithIrm } from '@jbl/wasm-lib'

export type Category = "stablecoin" | "volatile" | "lsd";

export interface Pool {
  id: string;
  address: string;
  name: string;
  symbol: string;
  icon: string;
  /** Collateral token (deposited as security) */
  collateralSymbol: string;
  collateralIcon: string;
  /** Lend token (borrowed / supplied as LP) */
  lendSymbol: string;
  lendIcon: string;
  category: Category;
  binancePerp?: string;
  account: PoolWithIrm;
  supplyAPY: number;
  borrowAPY: number;
  totalSupplied: number;
  totalBorrowed: number;
  totalCollateral: number;
  utilization: number;
  availableLiquidity: number;
}

export interface MultiplyMeta {
  maxMultiplier: number;
  maxNetAPY: number;
  debtSymbol: string;
  debtIcon: string;
}
