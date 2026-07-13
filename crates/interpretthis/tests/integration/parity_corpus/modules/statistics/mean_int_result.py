# Pins statistics.mean over all-int input whose true mean is itself an int:
# CPython returns int (not float), and printing reflects that.
import statistics

print(statistics.mean([1, 2, 3]))
