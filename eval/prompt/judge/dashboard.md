# Dashboard

Request: A sales dashboard: a page header 'Q3 overview', three stat tiles (Revenue $1.2M up 8%, Orders 3,420 up 3%, Refunds 41 down 12%), and below them a table of the five most recent orders (order id, customer, amount, status) bound to the data model.

## Sample A

```
Page title="Q3 overview"
  Stack direction="vertical" gap="lg"
    Grid columns={base: 1, md: 3} gap="md"
      StatTile label="Revenue" value="$1.2M" delta="+8%" tone="success"
      StatTile label="Orders" value="3,420" delta="+3%" tone="success"
      StatTile label="Refunds" value="41" delta="-12%" tone="success"
    Card title="Recent orders" padded=false
      Table rowKey="id" rows=$/orders columns[4: "Order ID" | "Customer" | "Amount" | "Status"]
[data model] /orders = [{"id":"#10234","customer":"Ava Thompson","amount":249,"status":"Paid"},{"id":"#10233","customer":"Liam Chen","amount":89.5,"status":"Shipped"},{"id":"#10232","customer":"Sofia Rossi","amount":1250,"status":"Paid"},{"id":"#10231","customer":"Noah Williams","amount":64.99,"status":"Refunded"},{"id":"#10230","customer":"Mia Johnson","amount":412.75,"status":"Pending"}]
```

## Sample B

```
Page title="Q3 overview"
  Grid columns={base: 1, sm: 3} gap="md"
    StatTile label="Revenue" value="$1.2M" delta="+8%" tone="success"
    StatTile label="Orders" value="3,420" delta="+3%" tone="success"
    StatTile label="Refunds" value="41" delta="-12%" tone="success"
  Card title="Recent orders"
    Table columns[4: "Order ID" | "Customer" | "Amount" | "Status"] rows=$/orders rowKey="id" caption="Five most recent orders"
[data model] {"orders":[{"id":"#10482","customer":"Acme Corp","amount":1280.5,"status":"Paid"},{"id":"#10481","customer":"Globex","amount":645,"status":"Pending"},{"id":"#10480","customer":"Initech","amount":2310.75,"status":"Paid"},{"id":"#10479","customer":"Umbrella Ltd","amount":189.99,"status":"Refunded"},{"id":"#10478","customer":"Stark Industries","amount":4520,"status":"Paid"}]}
```
