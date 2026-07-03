import { RootLayout } from "@/layouts/RootLayout";
import { CreatePoolPage } from "@/pages/CreatePoolPage";
import { MarketPage } from "@/pages/MarketPage";
import { MultiplyDetailPage } from "@/pages/MultiplyDetailPage";
import { MultiplyPage } from "@/pages/MultiplyPage";
import { PoolDetailPage } from "@/pages/PoolDetailPage";
import { PortfolioPage } from "@/pages/PortfolioPage";
import { BrowserRouter, Navigate, Route, Routes } from "react-router";

function App() {
  return (
    <BrowserRouter
      basename={import.meta.env.VITE_ROUTER_BASENAME ?? import.meta.env.BASE_URL}
    >
      <Routes>
        <Route element={<RootLayout />}>
          <Route index element={<Navigate to="/markets" replace />} />
          <Route path="/markets" element={<MarketPage />} />
          <Route path="/pool/create" element={<CreatePoolPage />} />
          <Route path="/pool/:address" element={<PoolDetailPage />} />
          <Route path="/multiply" element={<MultiplyPage />} />
          <Route path="/multiply/:id" element={<MultiplyDetailPage />} />
          <Route path="/portfolio" element={<PortfolioPage />} />
        </Route>
      </Routes>
    </BrowserRouter>
  );
}

export default App;
