export const metadata = {
  title: "FAQ and glossary",
  description:
    "Answers on lost keys, read-only connections, privacy mode, unavailable metrics and the terms used throughout the Linvesther documentation.",
};

const faq = [
  [
    "What if I lose my password or device?",
    "Nobody can restore the identity, including Linvesther, because no one else holds your key. Keep the backup file you can download when you create a password identity. With a passkey, your device or its keychain sync is the way back in.",
  ],
  [
    "Can Linvesther move or trade my money?",
    "No. Connections are read-only and keys with trading or withdrawal permissions are rejected. Your assets stay at your exchange.",
  ],
  [
    "Can I hide an account that did badly?",
    "No. All connected accounts are combined and you cannot leave one out. You can turn on privacy mode to hide the whole record.",
  ],
  [
    "Why is a metric unavailable?",
    "There is not enough history yet, or there is an unexplained gap. Unavailable is not zero.",
  ],
  [
    "Is a claim a zero-knowledge proof?",
    "Not yet. A claim is authorized by your signature and checked against figures read from your accounts. Proofs for claims are on the roadmap.",
  ],
  [
    "Why a test network?",
    "The registry runs on Base Sepolia while the protocol is reviewed. A production deployment is a separate step.",
  ],
  [
    "Does Linvesther store my API keys?",
    "Keys are used to read your history and are never published or shown. You can delete the key at your exchange whenever you like.",
  ],
];

const glossary = [
  [
    "Identity",
    "Your public key and the smart account that represents it. It is how you sign in and authorize things.",
  ],
  [
    "Connected account",
    "An exchange or broker account linked with a read-only credential.",
  ],
  [
    "Public profile",
    "The combined percentages anyone can read at your address. Off in privacy mode.",
  ],
  [
    "Privacy mode",
    "A switch that hides the whole public record. Anyone opening your address sees only that it is private.",
  ],
  [
    "Claim",
    "One statement about your record, signed by you and shared as a link.",
  ],
  [
    "Proof",
    "A zero-knowledge proof that a result follows from hidden data by a published method.",
  ],
  ["Drawdown", "The fall from a peak to a later low, as a percentage."],
  [
    "Time-weighted return",
    "Return that ignores the timing and size of deposits and withdrawals.",
  ],
  [
    "Relayer",
    "Whoever pays the fee to submit a signed action to the blockchain. It gains no authority.",
  ],
  [
    "Base",
    "The Ethereum layer-2 network where ownership and registration are recorded.",
  ],
];

export default function Faq() {
  return (
    <>
      <span className="eyebrow">Reference</span>
      <h1>FAQ and glossary</h1>
      <p className="document-lead">
        Short answers to common questions, and the terms used across Linvesther.
      </p>

      <section>
        <h2>Questions</h2>
        {faq.map(([question, answer]) => (
          <div key={question} className="docs-qa">
            <h3>{question}</h3>
            <p>{answer}</p>
          </div>
        ))}
      </section>

      <section>
        <h2>Glossary</h2>
        <table className="dimension-table">
          <tbody>
            {glossary.map(([term, meaning]) => (
              <tr key={term}>
                <td>{term}</td>
                <td>{meaning}</td>
              </tr>
            ))}
          </tbody>
        </table>
      </section>
    </>
  );
}
