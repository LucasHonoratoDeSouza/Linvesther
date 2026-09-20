import { brokerById } from "./brokers";
import styles from "./portfolio.module.css";

/** A broker's real logo  — or its first letter for a
 * broker that has none. */
export function BrokerLogo({ brokerId, size = 34 }: { brokerId: string; size?: number }) {
  const broker = brokerById(brokerId);
  return (
    <span className={styles.brokerLogo} style={{ width: size, height: size }}>
      {broker ? (
        // eslint-disable-next-line @next/next/no-img-element
        <img
          src={broker.logo}
          alt={broker.name}
          width={Math.round(size * 0.8)}
          height={Math.round(size * 0.8)}
          style={broker.roundedLogo ? { borderRadius: "22%" } : undefined}
        />
      ) : (
        brokerId.slice(0, 1).toUpperCase()
      )}
    </span>
  );
}
