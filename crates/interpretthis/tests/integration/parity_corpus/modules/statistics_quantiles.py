import statistics
print(statistics.quantiles([1, 2, 3, 4, 5]))
print(statistics.quantiles(range(1, 21)))
print(statistics.quantiles([1, 2, 3, 4, 5], n=10))
print(statistics.quantiles([1, 2, 3, 4, 5], method="inclusive"))
print(statistics.fmean([1, 2, 3, 4]))
