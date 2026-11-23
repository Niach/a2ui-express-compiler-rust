# Product card grid

Request: A product catalogue: a responsive grid of four product cards, each with an image, name, price, a rating, and an 'Add to cart' button; a search field above the grid.

## Sample A

```
Page title="Products"
  Stack
    Input label="Search products" placeholder="Search products" value={"@path":"/query"} name="search" type="search"
    Grid
[template card: repeated once per item of its list]
  Card
    Stack
      Image
      Heading text={"@path":"name"}
      Text text={"@path":"priceLabel"} variant="lead"
      Rating value={"@path":"rating"}
      Button label="Add to cart" icon="shopping-cart"
[data model] / = {"query":"","products":[{"id":"p1","name":"Trail Running Shoes","priceLabel":"$89.00","rating":4.5,"image":"https://images.example.com/products/trail-shoes.jpg"},{"id":"p2","name":"Insulated Water Bottle","priceLabel":"$24.50","rating":4,"image":"https://images.example.com/products/water-bottle.jpg"},{"id":"p3","name":"Wireless Headphones","priceLabel":"$129.99","rating":5,"image":"https://images.example.com/products/headphones.jpg"},{"id":"p4","name":"Canvas Backpack","priceLabel":"$59.00","rating":3.5,"image":"https://images.example.com/products/backpack.jpg"}]}
```

## Sample B

```
Page title="Products"
  Stack
    Input label="Search products" placeholder="Search products" value={"@path":"/search"} name="search" type="search"
    Grid
[template productCard: repeated once per item of its list]
  Card
    Image
    Text text={"@path":"name"} variant="title"
    Text text={"@path":"price"} variant="lead"
    Rating value={"@path":"rating"}
    Button label="Add to cart" icon="shopping-cart"
[data model] {"search":"","products":[{"name":"Trail Backpack","price":"$89.00","rating":4.5,"image":"https://example.com/img/backpack.jpg"},{"name":"Insulated Bottle","price":"$24.00","rating":4,"image":"https://example.com/img/bottle.jpg"},{"name":"Merino Beanie","price":"$32.00","rating":5,"image":"https://example.com/img/beanie.jpg"},{"name":"Camp Lantern","price":"$45.00","rating":3.5,"image":"https://example.com/img/lantern.jpg"}]}
```
