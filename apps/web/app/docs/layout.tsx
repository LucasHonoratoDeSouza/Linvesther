import { SiteFooter } from "../../components/SiteFooter";
import { DocsPager, DocsSidebar } from "./DocsNav";

export const metadata = { title: "Documentation" };

export default function DocsLayout({
  children,
}: {
  children: React.ReactNode;
}) {
  return (
    <>
      <main className="container docs-layout">
        <DocsSidebar />
        <article className="document">
          {children}
          <DocsPager />
        </article>
      </main>
      <SiteFooter />
    </>
  );
}
