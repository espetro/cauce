from typing import Any, Optional

from pydantic import BaseModel


class ContentsModel(BaseModel):
    text: Any = None
    highlights: Any = None
    summary: Any = None

    model_config = {"extra": "allow"}


class ExaRequest(BaseModel):
    query: str
    type: Optional[str] = "auto"
    numResults: Optional[int] = 10
    category: Optional[str] = None
    userLocation: Optional[str] = None
    includeDomains: Optional[list[str]] = None
    excludeDomains: Optional[list[str]] = None
    startPublishedDate: Optional[str] = None
    endPublishedDate: Optional[str] = None
    contents: Optional[ContentsModel] = None
    additionalQueries: Optional[list[str]] = None
    systemPrompt: Optional[str] = None
    outputSchema: Optional[dict] = None
    stream: Optional[bool] = None

    model_config = {"extra": "allow"}
