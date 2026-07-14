import statistics
try:
    print(statistics.median([1, 2, "x"]))
except TypeError as e:
    print("median:", type(e).__name__)
