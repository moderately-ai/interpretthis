# A set literal with an unhashable element raises TypeError. Regression: when
# value_to_key failed on an unhashable candidate, the `is_ok_and` returned false
# and the element was pushed into the set anyway — `{[1, 2]}` produced a set
# containing a list.
try:
    x = {[1, 2]}
    print("NO ERROR", x)
except TypeError:
    print("TypeError")

try:
    y = {1, {2: 3}}
    print("NO ERROR", y)
except TypeError:
    print("TypeError")

# Hashable set literals still build and dedup normally.
print(sorted({1, 2, 2, 3}))
print(sorted({"a", "b", "a"}))
