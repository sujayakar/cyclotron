import time

with open('test2.log') as f, open('test.log', 'w') as g:
    raw_input()
    for line in f:
        g.write(line)
        g.flush()
        time.sleep(0.1)
