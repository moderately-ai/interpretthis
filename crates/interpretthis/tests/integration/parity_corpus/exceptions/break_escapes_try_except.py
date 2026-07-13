# Pins: a bare `break` inside a try/except is NOT a caught exception;
# it just terminates the enclosing for loop. The except clause must not run.
result = "none"
for i in range(10):
    try:
        if i == 5:
            break
    except:
        result = "wrongly caught"
result = str(i)
print(result)
