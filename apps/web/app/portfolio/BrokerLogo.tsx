import Image from "next/image";
import { brokerById } from "./brokers";
import styles from "./portfolio.module.css";

/** A broker's real logo  — or its first letter for a
 * broker that has none. */
export function BrokerLogo({ brokerId, size = 34 }: { brokerId: string; size?: number }) {
  const broker = brokerById(brokerId);
  const logoSize = Math.round(size * 0.8);
  return (
    <span className={styles.brokerLogo} style={{ width: size, height: size }}>
      {broker ? (
        <Image
          src={broker.logo}
          alt={broker.name}
          width={logoSize}
          height={logoSize}
          style={broker.roundedLogo ? { borderRadius: "22%" } : undefined}
        />
      ) : (
        brokerId.slice(0, 1).toUpperCase()
      )}
    </span>
  );
}
