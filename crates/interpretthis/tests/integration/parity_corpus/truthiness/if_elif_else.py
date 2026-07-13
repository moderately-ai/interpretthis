# Pins: if/elif/else chain takes the first true branch.
x = 10
if x > 5:
    result = "big"
elif x > 0:
    result = "small"
else:
    result = "zero"
print(result)
