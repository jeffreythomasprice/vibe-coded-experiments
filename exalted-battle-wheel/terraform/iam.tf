data "aws_iam_policy_document" "server_access_codes" {
  statement {
    actions   = ["dynamodb:GetItem", "dynamodb:PutItem", "dynamodb:UpdateItem", "dynamodb:DeleteItem", "dynamodb:Scan"]
    resources = [aws_dynamodb_table.access_codes.arn]
  }
}

# A static key, not the instance role: the k0s node's IMDS hop limit is 1 (see
# ../kubernetes-host/terraform/instance.tf), deliberately so pods can't reach it. The key is
# exported as a sensitive output and lands in the Kubernetes Secret `deploy.sh` creates -- see
# CLAUDE.md's "Server hosting" section.
resource "aws_iam_user" "server" {
  name = "exalted-battle-wheel-server"
}

resource "aws_iam_user_policy" "server_access_codes" {
  name   = "access-codes"
  user   = aws_iam_user.server.name
  policy = data.aws_iam_policy_document.server_access_codes.json
}

resource "aws_iam_access_key" "server" {
  user = aws_iam_user.server.name
}
