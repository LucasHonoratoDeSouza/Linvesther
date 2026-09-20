import { Brand } from "./Brand";
import { Icon } from "./Icon";

export function SiteFooter() {
  return (
    <footer className="site-footer">
      <div className="container">
        <div className="footer-main">
          <div>
            <Brand />
            <p>
              Performance speaks.
              <br />
              Proof makes it matter.
            </p>
          </div>
          <div>
            <span className="eyebrow">Product</span>
            <a href="/portfolio">Portfolio</a>
            <a href="/disclose">Create a claim</a>
            <a href="/explorer">Explorer</a>
          </div>
          <div>
            <span className="eyebrow">Resources</span>
            <a href={process.env.NEXT_PUBLIC_DOCS_URL || "/docs"}>
              Documentation <Icon name="diagonal" width={12} />
            </a>
            <a href="/whitepaper">Whitepaper</a>
            <a href="/docs/concepts#verification">Verification model</a>
          </div>
          <div>
            <span className="eyebrow">Your account</span>
            <a href="/onboarding">Get started</a>
            <a href="/docs/claims">Privacy & disclosure</a>
          </div>
        </div>
        <div className="footer-bottom">
          <span>© {new Date().getFullYear()} Linvesther</span>
          <span>Built on evidence.</span>
          <a href="#main-content">Back to top ↑</a>
        </div>
      </div>
    </footer>
  );
}
