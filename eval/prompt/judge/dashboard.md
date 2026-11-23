# Dashboard

Request: A sales dashboard: a page header 'Q3 overview', three stat tiles (Revenue $1.2M up 8%, Orders 3,420 up 3%, Refunds 41 down 12%), and below them a table of the five most recent orders (order id, customer, amount, status) bound to the data model.

## Sample A

```
Page title="Q3 overview"
  Stack
    Grid
      StatTile label="Revenue" value="$1.2M"
      StatTile label="Orders" value="3,420"
      StatTile label="Refunds" value="41"
    Card title="Recent orders"
      Table columns[4]
[data model] /orders = [{"id":"ORD-3420","customer":"Acme Corp","amount":1250,"status":"Paid"},{"id":"ORD-3419","customer":"Globex","amount":480.5,"status":"Pending"},{"id":"ORD-3418","customer":"Initech","amount":9120,"status":"Paid"},{"id":"ORD-3417","customer":"Umbrella Ltd","amount":260.75,"status":"Refunded"},{"id":"ORD-3416","customer":"Hooli","amount":3340,"status":"Paid"}]
```

## Sample B

```
Page title="Q3 overview"
  Grid
    StatTile label="Revenue" value="$1.2M"
    StatTile label="Orders" value="3,420"
    StatTile label="Refunds" value="41"
  Table columns[4]
[data model] {"orders":[{"id":"#10482","customer":"Ava Thompson","amount":128.5,"status":"Paid"},{"id":"#10481","customer":"Liam Carter","amount":342,"status":"Paid"},{"id":"#10480","customer":"Noah Patel","amount":59.9,"status":"Pending"},{"id":"#10479","customer":"Mia Rossi","amount":214.25,"status":"Refunded"},{"id":"#10478","customer":"Ethan Kim","amount":87,"status":"Paid"}]}
```
