# itertools.product(repeat=) and accumulate(initial=) honour their keyword
# arguments. Regression: the module call path dropped kwargs, so repeat was 1
# and initial was ignored.
import itertools

print(list(itertools.product([1, 2], repeat=2)))
print(list(itertools.product([1, 2], ["a", "b"], repeat=2)))
print(list(itertools.product([1, 2], repeat=1)))
print(list(itertools.accumulate([1, 2, 3], initial=10)))
print(list(itertools.accumulate([1, 2, 3, 4], lambda a, b: a * b, initial=2)))
print(list(itertools.accumulate([], initial=5)))     # just the initial

# Unknown keyword to product raises TypeError.
try:
    list(itertools.product([1], foo=2))
except TypeError:
    print("TypeError")
