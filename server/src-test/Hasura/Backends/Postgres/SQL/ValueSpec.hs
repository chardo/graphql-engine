module Hasura.Backends.Postgres.SQL.ValueSpec (spec) where

import Hasura.Backends.Postgres.SQL.DML
import Hasura.Backends.Postgres.SQL.Types
import Hasura.Backends.Postgres.SQL.Value
import Hasura.Base.Error (Code (ParseFailed), QErr (..), runAesonParser)
import Hasura.Prelude
import Network.HTTP.Types qualified as HTTP
import Test.Hspec

spec :: Spec
spec = do
  txtEncoderSpec
  jsonValueSpec

singleElement, multiElement, nestedArray, nestedArray', malformedArray :: PGScalarValue
singleElement = PGValArray [PGValInteger 1]
multiElement = PGValArray [PGValVarchar "a", PGValVarchar "b"]
nestedArray = PGValArray [multiElement, multiElement]
nestedArray' = PGValArray [nestedArray]
malformedArray = PGValArray [PGValInteger 1]

txtEncoderSpec :: Spec
txtEncoderSpec =
  describe "txtEncoder should encode a valid Postgres array of:" $ do
    it "a single element" $ do
      txtEncoder singleElement `shouldBe` SELit "{1}"
    it "multiple elements" $ do
      txtEncoder multiElement `shouldBe` SELit "{a,b}"
    it "simple nested arrays" $ do
      txtEncoder nestedArray `shouldBe` SELit "{{a,b},{a,b}}"
    it "more deeply nested arrays" $ do
      txtEncoder nestedArray' `shouldBe` SELit "{{{a,b},{a,b}}}"

pgArrayRoundtrip :: PGScalarValue -> PGScalarType -> Expectation
pgArrayRoundtrip v t = do
  let parsedValue = runExcept $ runAesonParser (parsePGValue t) (pgScalarValueToJson v)
  parsedValue `shouldBe` Right v

jsonValueSpec :: Spec
jsonValueSpec = describe "JSON Roundtrip: PGArray" do
  describe "parsePGValue" $ do
    it "singleElement PGArray" $ do
      pgArrayRoundtrip singleElement (PGArray PGInteger)
    it "multiElement PGArray" $ do
      pgArrayRoundtrip multiElement (PGArray PGVarchar)
    it "nestedArray PGArray" $ do
      pgArrayRoundtrip nestedArray (PGArray (PGArray PGVarchar))
    it "nestedArray' PGArray" $ do
      pgArrayRoundtrip nestedArray' (PGArray (PGArray (PGArray PGVarchar)))
    it "malformedArray PGArray" $ do
      let parsedValue = runExcept $ runAesonParser (parsePGValue PGVarchar) (pgScalarValueToJson malformedArray)
      parsedValue
        `shouldBe` Left
          QErr
            { qePath = [],
              qeStatus =
                HTTP.Status
                  { statusCode = 400,
                    statusMessage = "Bad Request"
                  },
              qeError = "parsing Text failed, expected String, but encountered Array",
              qeCode = ParseFailed,
              qeInternal = Nothing
            }
