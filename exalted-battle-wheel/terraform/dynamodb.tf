resource "aws_dynamodb_table" "access_codes" {
  name         = local.access_codes_table
  billing_mode = "PAY_PER_REQUEST"
  hash_key     = "access_key"

  # DynamoDB is schemaless outside the key: `is_admin` and `created_at` need no declaration here.
  attribute {
    name = "access_key"
    type = "S"
  }
}
