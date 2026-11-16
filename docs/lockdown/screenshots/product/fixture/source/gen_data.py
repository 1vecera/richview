"""Write the synthetic CSV inputs of the lockdown fixture report with a fixed seed."""
import csv
import random
import sys

out = sys.argv[1]
rng = random.Random(20261021)
with open(f"{out}/data/latency.csv", "w", newline="") as handle:
    writer = csv.writer(handle)
    writer.writerow(["call", "ms"])
    for index in range(240):
        value = rng.lognormvariate(5.2, 0.42)
        writer.writerow([index + 1, round(min(value, 980.0), 1)])
segments = [("Self-serve", 30, 1.3, 0.35), ("SMB", 30, 2.1, 0.3), ("Mid-market", 25, 2.7, 0.3), ("Enterprise", 20, 3.4, 0.45)]
with open(f"{out}/data/onboarding.csv", "w", newline="") as handle:
    writer = csv.writer(handle)
    writer.writerow(["account", "segment", "days"])
    count = 0
    for name, size, mu, sigma in segments:
        for _ in range(size):
            count += 1
            writer.writerow([f"A{count:03d}", name, max(1, round(rng.lognormvariate(mu, sigma)))])
