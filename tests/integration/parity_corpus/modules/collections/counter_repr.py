# Counter print/repr produces "Counter({'a': 2, 'b': 1})" matching
# CPython exactly. Pins types::Value::Counter Display impl. Closes
# the pre-B3 divergence where Counter was a thin dict shim and
# printed as a plain dict.
import collections
print(collections.Counter('aabbc'))
print(collections.Counter('hello'))
print(collections.Counter([1, 2, 1, 3, 2, 1]))
print(collections.Counter(''))
print(collections.Counter())
print(collections.Counter((1, 2, 1, 3)))
