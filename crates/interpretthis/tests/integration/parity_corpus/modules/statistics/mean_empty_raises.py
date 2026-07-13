# statistics.mean rejects an empty input with StatisticsError; both engines
# must raise so the runner sees matching non-zero exit status.
import statistics

print(statistics.mean([]))
