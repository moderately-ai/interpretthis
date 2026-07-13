# itertools.compress + accumulate.
#
# Pins CPython semantics:
#   - compress(data, selectors) yields data[i] for truthy selectors[i];
#     trailing selectors past data are ignored, and vice versa.
#   - accumulate(iter) defaults to running addition; passing a callable
#     folds with it (e.g. operator.mul).
from itertools import compress, accumulate

# compress: simple bitmap-style selection.
print(list(compress("ABCDEF", [1, 0, 1, 0, 1, 1])))
print(list(compress([1, 2, 3, 4], [True, False, True, True])))
print(list(compress([1, 2, 3], [1, 1])))     # selectors shorter
print(list(compress([1, 2], [1, 1, 1, 1])))  # data shorter

# accumulate: defaults to addition.
print(list(accumulate([1, 2, 3, 4, 5])))      # [1, 3, 6, 10, 15]
print(list(accumulate([10, -3, 2, -7])))      # running sum
print(list(accumulate([])))                    # empty
print(list(accumulate([42])))                  # single

# accumulate with a custom binary callable.
print(list(accumulate([1, 2, 3, 4], lambda a, b: a * b)))   # cumulative product
print(list(accumulate([5, 3, 8, 1, 4], lambda a, b: a if a > b else b)))  # running max
