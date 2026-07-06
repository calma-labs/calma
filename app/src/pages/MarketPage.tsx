import { MarketFilters } from "@/components/market/MarketFilters";
import {
  MarketTable,
  type SortDir,
  type SortKey,
} from "@/components/market/MarketTable";
import { useLendingAccounts } from "@/hooks/program/useLendingAccounts";
import { useMintDecimalsMap } from "@/hooks/useMintDecimals";
import { poolDataToDisplayPool } from "@/lib/poolDisplay";
import type { Category, Pool } from "@/types/pool";
import { BarChart3 } from "lucide-react";
import { PublicKey } from "@solana/web3.js";
import { useEffect, useMemo, useState } from "react";
import { isPoolValid } from "@/lib/validation";

export function MarketPage() {
  const { data: lendingAccounts = [], isLoading: isLoadingPools } = useLendingAccounts();
  const [search, setSearch] = useState("");
  const [categoryFilter, setCategoryFilter] = useState<"all" | Category>("all");
  const [sortKey, setSortKey] = useState<SortKey>("totalSupplied");
  const [sortDir, setSortDir] = useState<SortDir>("desc");
  const [validPoolIds, setValidPoolIds] = useState<Set<string>>(new Set());
  const [isCheckingMints, setIsCheckingMints] = useState(false);

  // Check which pools have valid minter
  useEffect(() => {
    if (lendingAccounts.length === 0) {
      setValidPoolIds(new Set());
      return;
    }
    
    setIsCheckingMints(true);
    Promise.all(
      lendingAccounts.map(async (pool) => {
        const isValid = await isPoolValid(pool);
        return { id: pool.publicKey.toBase58(), isValid };
      })
    ).then(results => {
      const validIds = new Set(results.filter(r => r.isValid).map(r => r.id));
      setValidPoolIds(validIds);
      setIsCheckingMints(false);
    });
  }, [lendingAccounts]);

  const isLoading = isLoadingPools || isCheckingMints;

  const lendMints = useMemo(
    () => lendingAccounts.map((pd) => new PublicKey(pd.account.lend_mint)),
    [lendingAccounts],
  );
  const decimalsMap = useMintDecimalsMap(lendMints);

  const allPools = useMemo<Pool[]>(
    () => lendingAccounts
      .filter(pool => validPoolIds.has(pool.publicKey.toBase58()))
      .map(pd => {
        const lendDecimals = decimalsMap.get(new PublicKey(pd.account.lend_mint).toBase58()) ?? 6;
        return poolDataToDisplayPool(pd.publicKey, pd.account, lendDecimals);
      }),
    [lendingAccounts, validPoolIds, decimalsMap],
  );

  function handleSort(key: SortKey) {
    if (sortKey === key) {
      setSortDir((d) => (d === "asc" ? "desc" : "asc"));
    } else {
      setSortKey(key);
      setSortDir("desc");
    }
  }

  const filteredPools = useMemo(
    () =>
      allPools
        .filter((p) => {
          const q = search.toLowerCase();
          const matchesSearch =
            p.name.toLowerCase().includes(q) ||
            p.symbol.toLowerCase().includes(q) ||
            p.id.toLowerCase().includes(q);
          return matchesSearch;
        })
        .sort((a, b) => {
          const mult = sortDir === "asc" ? 1 : -1;
          const val = (p: Pool) =>
            sortKey === "ltv" ? p.account.ltv_percent : (p[sortKey] as number);
          return mult * (val(a) - val(b));
        }),
    [allPools, search, categoryFilter, sortKey, sortDir],
  );

  return (
    <div className="w-full max-w-6xl mx-auto px-4 py-12">
      <div className="mb-8">
        <div className="flex items-center gap-2.5 mb-2">
          <BarChart3 className="h-5 w-5 text-surface-accent" />
          <h1 className="text-3xl font-semibold tracking-tight text-surface-foreground">
            Markets
          </h1>
        </div>
        <p className="text-sm text-surface-foreground/50 max-w-lg">
          Select an asset to supply, borrow, repay or withdraw. Rates update in
          real-time based on pool utilisation.
        </p>
      </div>

      <MarketFilters
        search={search}
        onSearchChange={setSearch}
        categoryFilter={categoryFilter}
        onCategoryChange={setCategoryFilter}
      />

      {isLoading ? (
        <div className="flex items-center justify-center py-32 text-surface-foreground/30 text-sm">
          Loading markets…
        </div>
      ) : (
        <MarketTable
          pools={filteredPools}
          sortKey={sortKey}
          sortDir={sortDir}
          onSort={handleSort}
        />
      )}
    </div>
  );
}
