# statistics.variance requires at least two data points (n-1 denominator);
# a single-element input raises StatisticsError.
import statistics

print(statistics.variance([5]))
