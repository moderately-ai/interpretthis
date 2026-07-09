# statistics.mean: all-int input whose true mean is non-integral returns a float;
# the printed form must carry the decimal point (`2.5`, not `2`).
import statistics

print(statistics.mean([1, 2, 3, 4]))
