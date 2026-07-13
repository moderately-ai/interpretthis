# Pins: `break` inside a while loop suppresses the else clause.
result = "none"
x = 0
while x < 10:
    x += 1
    if x == 5:
        break
else:
    result = "else_ran"
print(result)
