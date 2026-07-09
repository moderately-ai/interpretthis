# Pins: `while ... else:` runs the else clause when the condition becomes false.
x = 0
while x < 3:
    x += 1
else:
    result = "else_ran"
print(result)
