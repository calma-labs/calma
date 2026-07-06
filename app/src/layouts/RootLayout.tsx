import { Footer } from "@/components/layout/Footer";
import { Header } from "@/components/layout/Header";
import { Outlet } from "react-router";

export function RootLayout() {
  return (
    <div className="min-h-svh bg-surface font-sans flex flex-col">
      <Header />
      <main className="flex-1 px-4">
        <Outlet />
      </main>
      <Footer />
    </div>
  );
}
