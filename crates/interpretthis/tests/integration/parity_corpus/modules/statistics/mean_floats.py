# statistics.mean: float input stays float regardless of integral result;
# `mean([1.5, 2.5])` prints as `2.0`, not `2`.
import statistics

print(statistics.mean([1.5, 2.5]))
