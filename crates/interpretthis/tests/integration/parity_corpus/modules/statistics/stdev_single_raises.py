# statistics.stdev shares variance's n>=2 precondition: single-element input
# raises StatisticsError.
import statistics

print(statistics.stdev([42]))
