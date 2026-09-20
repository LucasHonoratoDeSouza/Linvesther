export const metadata = { title: "Contracts" };

const contracts = [
  [
    "IdentityRegistry",
    "0x4a24Ce35Fc8050600921Ff722FeE47bEd4963a99",
    "Identities, their tracks and owner rotation.",
  ],
  [
    "AccountRegistry",
    "0xCCd02127b75b2D025FCbb86d254977d5949034a0",
    "Which accounts are bound to a track, and when.",
  ],
  [
    "ProfileRegistry",
    "0xc7030f8b1Af00da36A2616a55dA69ECeDd2fc1AE",
    "The optional public name and bio.",
  ],
  [
    "Account factory",
    "0x31b565911Db40865B279E28f3f9A11FF84def40E",
    "Creates the smart account that represents an identity.",
  ],
];

const events = [
  [
    "IdentityRegistry",
    "IdentityCreated, TrackCreated, OwnerRotationProposed, OwnerRotated",
  ],
  [
    "AccountRegistry",
    "AccountRegistered, AccountActivated, AccountRemoved, MembershipChanged",
  ],
  ["ProfileRegistry", "ProfileUpdated"],
];

export default function ContractsDocs() {
  return (
    <>
      <span className="eyebrow">Reference</span>
      <h1>Contracts</h1>
      <p className="document-lead">
        Ownership and registration are recorded on Base. You can read all of it
        directly from the chain, without asking Linvesther.
      </p>
      <div className="document-notice">
        These are deployed on <strong>Base Sepolia</strong> (chain ID 84532), a
        test network. Addresses will change when a production deployment is
        made.
      </div>

      <section>
        <h2>Deployed addresses</h2>
        <table className="dimension-table">
          <thead>
            <tr>
              <th>Contract</th>
              <th>Address</th>
              <th>Purpose</th>
            </tr>
          </thead>
          <tbody>
            {contracts.map(([name, address, purpose]) => (
              <tr key={name}>
                <td>{name}</td>
                <td>
                  <a
                    href={`https://sepolia.basescan.org/address/${address}`}
                    target="_blank"
                    rel="noreferrer"
                  >
                    <code>{address}</code>
                  </a>
                </td>
                <td>{purpose}</td>
              </tr>
            ))}
          </tbody>
        </table>
      </section>

      <section>
        <h2>Properties</h2>
        <ul>
          <li>
            <strong>Immutable.</strong> There is no upgrade proxy. The rules
            cannot be changed by whoever runs the service.
          </li>
          <li>
            <strong>Signature-authorized.</strong> A state change needs the
            owner&apos;s signature, checked on-chain. It works for ordinary key
            signatures and for smart-account signatures (ERC-1271).
          </li>
          <li>
            <strong>Any submitter.</strong> Whoever broadcasts the transaction
            pays the fee and gains no authority. The owner, or any other
            relayer, can submit the same signed command.
          </li>
          <li>
            <strong>Nothing about your trading.</strong> Only identities,
            bindings, optional profile text and proofs are recorded.
          </li>
        </ul>
      </section>

      <section>
        <h2>Events</h2>
        <p>The full history can be rebuilt from these events:</p>
        <table className="dimension-table">
          <thead>
            <tr>
              <th>Contract</th>
              <th>Events</th>
            </tr>
          </thead>
          <tbody>
            {events.map(([name, list]) => (
              <tr key={name}>
                <td>{name}</td>
                <td>
                  <code>{list}</code>
                </td>
              </tr>
            ))}
          </tbody>
        </table>
        <p>
          Profile text is public and its history stays in the event log, so an
          earlier version can still be read after an update.
        </p>
      </section>
    </>
  );
}
