# Pins: itertools.takewhile accepts an unbound type method (str.isdigit)
# as the predicate. Routes through the shared call_callable helper
# which only handles Function/Lambda today.
import itertools
print(list(itertools.takewhile(str.isdigit, ['1', '2', '3', 'x', '4'])))
