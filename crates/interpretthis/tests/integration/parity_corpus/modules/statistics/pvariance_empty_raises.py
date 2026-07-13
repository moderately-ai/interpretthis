# statistics.pvariance rejects an empty input with StatisticsError, even
# though n=1 is allowed.
import statistics

print(statistics.pvariance([]))
