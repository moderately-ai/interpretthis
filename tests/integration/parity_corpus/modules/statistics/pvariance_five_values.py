# statistics.pvariance over five ints (population variance, n denominator);
# differs from variance(...) by the (n-1)/n factor.
import statistics

print(statistics.pvariance([1, 2, 3, 4, 5]))
