import { useWalletConnection } from "@solana/react-hooks";
import { useEffect, useRef } from "react";
import { useWalletBalancesStore } from "../store/wallet.store";

const REFRESH_INTERVAL_MS = 15_000;

/**
 * Headless component that keeps the wallet-balance Zustand store in sync.
 *
 * - Fetches balances immediately when the wallet connects.
 * - Re-fetches every 15 seconds while connected.
 * - Clears the store when the wallet disconnects.
 *
 * Mount this once inside Providers — it renders nothing.
 */
export function WalletBalancesSync() {
  const { connected, wallet } = useWalletConnection();
  const { fetch, clear } = useWalletBalancesStore();
  const intervalRef = useRef<ReturnType<typeof setInterval> | null>(null);

  const address = connected && wallet ? String(wallet.account.address) : null;

  useEffect(() => {
    if (!address) {
      clear();
      if (intervalRef.current) clearInterval(intervalRef.current);
      return;
    }

    void fetch(address);

    intervalRef.current = setInterval(
      () => void fetch(address),
      REFRESH_INTERVAL_MS,
    );

    return () => {
      if (intervalRef.current) clearInterval(intervalRef.current);
    };
  }, [address, fetch, clear]);

  return null;
}
