# statistics.mode preserves element type — string input returns the most
# frequent string, not its count.
import statistics

print(statistics.mode(["a", "b", "a"]))
