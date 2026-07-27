import { queryClient } from "@/config/queryClient";
import { SolanaProvider } from "@solana/react-hooks";
import { QueryClientProvider } from "@tanstack/react-query";
import React from "react";
import { ToastContainer } from "react-toastify";
import { WalletBalancesSync } from "./components/WalletBalancesSync";
import { solanaClient } from "./config/solana";

export function Providers({ children }: { children: React.ReactNode }) {
  return (
    <QueryClientProvider client={queryClient}>
      <SolanaProvider client={solanaClient}>
        <WalletBalancesSync />
        {children}
        <ToastContainer
          position="bottom-right"
          theme="colored"
          closeOnClick
          pauseOnHover
          draggable={false}
          toastClassName="calma-toast"
        />
      </SolanaProvider>
    </QueryClientProvider>
  );
}
