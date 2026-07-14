from datetime import datetime, timedelta, date
d = date(2020, 2, 28)
print(d + timedelta(days=1))
print(d + timedelta(days=2))
dt = datetime(2020, 1, 1, 12, 30)
print((dt + timedelta(hours=25)).isoformat())
print((date(2020, 3, 1) - date(2020, 2, 1)).days)
print(date(2020, 1, 15).weekday())
print(d.strftime("%Y-%m-%d"))
