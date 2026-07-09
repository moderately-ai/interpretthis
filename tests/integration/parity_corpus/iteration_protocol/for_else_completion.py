# Pins: `for: ... else: ...` runs the else block when the loop drains
# without break.
result = "none"
for i in range(3):
    pass
else:
    result = "else_ran"
print(result)
