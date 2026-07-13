# Pins: `break` inside a for-loop suppresses the else clause.
result = "none"
for i in range(3):
    if i == 1:
        break
else:
    result = "else_ran"
print(result)
