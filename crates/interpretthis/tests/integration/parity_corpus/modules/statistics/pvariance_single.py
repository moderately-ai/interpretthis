# statistics.pvariance on a single-element input is zero (population variance
# accepts n=1; sample variance does not).
import statistics

print(statistics.pvariance([1]))
