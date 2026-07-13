# Pins: finally fires on every iteration that enters the try, including the
# one that breaks out of the for loop.
log = []
for i in range(3):
    try:
        if i == 1:
            break
    finally:
        log.append(str(i))
print(",".join(log))
