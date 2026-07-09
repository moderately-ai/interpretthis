# statistics.mode tie-break: when multiple values share the max count, CPython
# returns whichever appeared first in the input (1 here, not 2).
import statistics

print(statistics.mode([1, 1, 2, 2, 3]))
