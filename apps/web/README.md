# LinvestherZK web

Next.js application for the public site, account onboarding, portfolio,
claim disclosure and public track explorer. Motion drives the interactive
illustrations and transitions; reduced-motion preferences are respected.

## Run

```sh
pnpm --filter @linvestherzk/web dev
pnpm --filter @linvestherzk/web typecheck
pnpm --filter @linvestherzk/web build
```

The development server listens on port 4300. The API is configured separately.

| Variable | Default | Purpose |
| --- | --- | --- |
| `NEXT_PUBLIC_API_URL` | `http://localhost:4301` | API origin |
| `NEXT_PUBLIC_SIWE_DOMAIN` | `localhost` | Wallet sign-in domain |
| `NEXT_PUBLIC_DOCS_URL` | `/docs` | Header and footer documentation destination |

A documentation subdomain can be supplied through `NEXT_PUBLIC_DOCS_URL`
after that site and its DNS are deployed. Setting the variable only changes
navigation links; it does not create a subdomain.

## Routes

| Route | Purpose |
| --- | --- |
| `/` | Product overview and interactive selective-disclosure explainer |
| `/portfolio` | Wallet sign-in, read-only Binance connection and actual performance |
| `/onboarding` | Identity creation, account binding and removal |
| `/disclose` | Wallet-authorized public claim publication |
| `/explorer` | Lookup by Track ID or shared public profile URL |
| `/profile/[trackId]` | Public projection, five dimensions, gaps and corrections |
| `/docs` | Introductory documentation and public API guide |
| `/whitepaper` | Clearly labeled draft protocol overview, with print styling |
| `/recover` | Account access guidance and explicit recovery availability |

The explorer does not support wallet-to-track lookup. Lost-key recovery and
key rotation are not exposed by the web interface. Publishing a signed claim
does not generate a ZK receipt. The landing-page examples explain the intended
concept and are explicitly marked as illustrative; they do not submit API jobs.

## Visual system and media

`app/globals.css` defines the shared colors, typography, responsive layouts,
application shell and print styles. Interactive product components live in
`components/` alongside the shared navigation.

The home-page film slot includes a visible production brief: a 60–90 second
1920 × 1080 product walkthrough, MP4/WebM, poster image and English captions.
Use synthetic account data and exclude credentials from the recording. The
current slot does not load a video or pretend that playback is available.

The animated hero artwork is native SVG and requires no external media.

## Browser verification

```sh
pnpm --filter e2e-tests test:web
```

The browser suite starts the web and API services, checks wallet signatures
with a synthetic test account and exercises navigation, disclosure, public
profiles, connection failures and narrow-screen interactions. Ports 4300 and
4301 must be free for the default configuration.
